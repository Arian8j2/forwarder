mod encryption;
mod peer;
mod poll;
pub mod socket;
pub mod uri;

use crate::socket::icmp::IcmpEchoType;
use anyhow::Context;
use parking_lot::{RwLock, RwLockUpgradableReadGuard, RwLockWriteGuard};
use poll::Poll;
use socket::icmp::ICMP_RESERVED_BYTES_LEN;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use {
    peer::{Peer, PeerManager},
    socket::Socket,
    uri::Uri,
};

// all buffers that are used for receiving and sending packet will use this size
const MAX_PACKET_SIZE: usize = 65535;

/// interval that cleanup happens, lowering this result in lower allowed unused time
const CLEANUP_INTERVAL: Duration = Duration::from_secs(7 * 60);

/// blocks current thread and runs a forwarder server that listens on `listen_uri` and forwards
/// all incoming packets to `remote_uri` and also forwards all packets returned by `remote_uri`
/// to the client that initiated the connection
///
/// # Error
/// this function only returns early errors, such as being unable to listen on `listen_uri` or
/// failing to create server `Poll` and ... it will panic on other late errors
pub fn run(
    listen_uri: Uri,
    remote_uri: Uri,
    passphrase: Option<String>,
    reverse_icmp: bool,
) -> anyhow::Result<()> {
    let listen_addr = &listen_uri.addr;
    let icmp_type = if reverse_icmp {
        IcmpEchoType::Reply
    } else {
        IcmpEchoType::Request
    };
    let socket = Socket::bind(listen_uri.protocol, listen_addr, icmp_type)
        .with_context(|| "couldn't create server")?;
    log::info!("listen on '{listen_addr}'");

    let poll = poll::new(remote_uri.protocol, remote_uri.addr, icmp_type.opposite())
        .with_context(|| "couldn't create poll")?;
    let registry = poll
        .get_registry()
        .with_context(|| "couldn't create poll registry")?;
    let peer_manager = Arc::new(RwLock::new(PeerManager::new(registry)));
    let socket = Arc::new(socket);
    spawn_peers_thread(
        poll,
        peer_manager.clone(),
        socket.clone(),
        passphrase.clone(),
    );
    spawn_cleanup_thread(peer_manager.clone());
    run_server(
        socket,
        peer_manager,
        passphrase,
        remote_uri,
        icmp_type.opposite(),
        listen_uri.addr.port(),
    );
    Ok(())
}

// creating buffer became complicated when i wanted to achieve zero allocation so a helper makes it easy
#[macro_export]
macro_rules! create_socket_buffer {
    ($size:expr) => {
        &mut [0u8; ICMP_RESERVED_BYTES_LEN + $size][ICMP_RESERVED_BYTES_LEN..]
    };
}

/// runs server in current thread
fn run_server(
    socket: Arc<Socket>,
    peer_manager: Arc<RwLock<PeerManager>>,
    passphrase: Option<String>,
    remote_uri: Uri,
    peer_icmp_type: IcmpEchoType,
    listening_port: u16,
) {
    let buffer = create_socket_buffer!(MAX_PACKET_SIZE);
    loop {
        let Ok((size, from_addr)) = socket.recv_from(buffer) else {
            continue;
        };
        if from_addr.port() == listening_port {
            // reverse healthcheck
            continue;
        }
        if let Some(ref passphrase) = passphrase {
            encryption::xor_encrypt(&mut buffer[..size], passphrase)
        }
        // lock needs to be upgrdable so when new peer appeared
        // it be able to append it to the peers list
        let peers = peer_manager.upgradable_read();
        match peers.find_peer_with_client_addr(&from_addr) {
            Some(peer) => {
                peer.set_used();
                // client ---> server socket ---peer socket----> remote
                peer.socket.send(&mut buffer[..size]).ok();
            }
            None => {
                log::info!("new client '{from_addr}'");
                let peers = RwLockUpgradableReadGuard::upgrade(peers);
                let peer = match add_new_peer(&remote_uri, from_addr, peers, peer_icmp_type) {
                    Ok(peer) => peer,
                    Err(error) => {
                        log::error!("couldn't add new peer: {error:?}");
                        continue;
                    }
                };
                // peer is just created so the `used` is true
                // and doesn't need to set it
                peer.socket.send(&mut buffer[..size]).ok();
            }
        };
    }
}

/// creates new `Peer` and appends it to the `PeerManager`
fn add_new_peer(
    remote_uri: &Uri,
    from_addr: SocketAddr,
    mut peers: RwLockWriteGuard<PeerManager>,
    icmp_echo_type: IcmpEchoType,
) -> anyhow::Result<Arc<Peer>> {
    let new_peer = Peer::new(remote_uri, from_addr, icmp_echo_type)?;
    let peer = peers.add_peer(new_peer)?;
    Ok(peer)
}

/// spawns peers_thread and panics if it exits
fn spawn_peers_thread(
    poll: Box<dyn Poll>,
    peers: Arc<RwLock<PeerManager>>,
    server_socket: Arc<Socket>,
    passphrase: Option<String>,
) {
    std::thread::spawn(|| {
        if let Err(error) = peers_thread(poll, peers, server_socket, passphrase) {
            log::error!("peers thread exited with error: {error:?}");
            panic!("peers thread exited")
        }
    });
}

/// blocks current thread and handles all incoming packets to each `Peer`
fn peers_thread(
    mut poll: Box<dyn Poll>,
    peers: Arc<RwLock<PeerManager>>,
    server_socket: Arc<Socket>,
    passphrase: Option<String>,
) -> anyhow::Result<()> {
    let on_peer_recv = Box::new(move |peer: &Peer, buffer: &mut [u8]| {
        peer.set_used();
        if let Some(ref passphrase) = passphrase {
            encryption::xor_encrypt(buffer, passphrase)
        }
        // client <--server socket--- peer <----- remote
        server_socket.send_to(buffer, peer.get_client_addr()).ok();
    });
    poll.poll(peers, on_peer_recv)?;
    Ok(())
}

/// spawns cleanup thread
fn spawn_cleanup_thread(peer_manager: Arc<RwLock<PeerManager>>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(CLEANUP_INTERVAL);
        try_cleanup(&peer_manager);
    });
}

/// tries to clean peers that has not been used for about `CLEANUP_INTERVAL` duration
fn try_cleanup(peer_manager: &RwLock<PeerManager>) {
    let mut peers = peer_manager.write();
    let mut used_client_count = 0;
    for peer in peers.get_all() {
        let used = peer.reset_used();
        if !used {
            let client_addr = *peer.get_client_addr();
            log::info!("cleaning peer that handled '{client_addr}'");
            if let Err(error) = peers.remove_peer(peer) {
                log::warn!("couldn't remove peer of '{client_addr}': {error:?}");
            }
        } else {
            used_client_count += 1;
        }
    }
    log::info!("{used_client_count} clients remaining after cleanup");
}
