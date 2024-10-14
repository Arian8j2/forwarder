mod encryption;
mod peer;
mod poll;
pub mod socket;

use parking_lot::{RwLock, RwLockUpgradableReadGuard, RwLockWriteGuard};
use poll::Poll;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use {
    peer::{Peer, PeerManager},
    socket::{Socket, SocketUri},
};

// all buffers that are used as recv buffer will have this size
const MAX_PACKET_SIZE: usize = 65535;

/// interval that cleanup happens, also lowering this result in lower allowed unused time
const CLEANUP_INTERVAL: Duration = Duration::from_secs(7 * 60);

/// runs a forwarder server that listens on `listen_uri` and forwards
/// all incoming packets to `remote_uri` and also forwards all packets
/// that `remote_uri` returns to client that initiated the connection
///
/// # Be careful
/// this function blocks the whole thread and doesn't stop until something panics
pub fn run_server(listen_uri: SocketUri, remote_uri: SocketUri, passphrase: Option<String>) {
    let listen_addr = &listen_uri.addr;
    let socket = Socket::bind(listen_uri.protocol, listen_addr)
        .unwrap_or_else(|_| panic!("couldn't listen on '{listen_addr}'"));
    let socket = Arc::new(socket);
    log::info!("listen on '{listen_addr}'");

    // TODO: better error handling in `run_server`
    let poll = poll::new(remote_uri.protocol, remote_uri.addr.is_ipv6())
        .unwrap_or_else(|err| panic!("couldn't create poll: {err:?}"));
    let registry = poll
        .get_registry()
        .unwrap_or_else(|err| panic!("couldn't create registry: {err:?}"));

    let peer_manager = PeerManager::new(registry).unwrap();
    let peer_manager: Arc<RwLock<PeerManager>> = Arc::new(RwLock::new(peer_manager));
    {
        let peer_manager = peer_manager.clone();
        let server_socket = socket.clone();
        let passphrase = passphrase.clone();
        std::thread::spawn(|| try_peers_thread(poll, peer_manager, server_socket, passphrase));
    }
    {
        let peer_manager = peer_manager.clone();
        std::thread::spawn(|| cleanup_thread(peer_manager));
    }

    let mut buffer = [0u8; MAX_PACKET_SIZE];
    loop {
        let Ok((size, from_addr)) = socket.recv_from(&mut buffer) else {
            continue;
        };

        if let Some(ref passphrase) = passphrase {
            encryption::xor_encrypt(&mut buffer[..size], passphrase)
        }
        // lock needs to be upgrdable so when new peer appeared
        // be able to append it to the peers list
        let peers = peer_manager.upgradable_read();
        match peers.find_peer_with_client_addr(&from_addr) {
            Some(peer) => {
                peer.set_used();
                // client ---> server socket ---peer socket----> remote
                peer.socket.send(&buffer[..size]).ok();
            }
            None => {
                log::info!("new client '{from_addr}'");
                let peers = RwLockUpgradableReadGuard::upgrade(peers);
                let peer = match add_new_peer(&remote_uri, from_addr, peers) {
                    Ok(peer) => peer,
                    Err(error) => {
                        log::error!("couldn't add new peer: {error:?}");
                        continue;
                    }
                };
                // peer is just created so the `used` is true
                // and doesn't need to set it
                peer.socket.send(&buffer[..size]).ok();
            }
        };
    }
}

/// prepare new `Peer` and add it to `PeerManager` and register it's epoll events
fn add_new_peer(
    remote_uri: &SocketUri,
    from_addr: SocketAddr,
    mut peers: RwLockWriteGuard<PeerManager>,
) -> anyhow::Result<Arc<Peer>> {
    let new_peer = Peer::new(remote_uri, from_addr)?;
    let peer = peers.add_peer(new_peer)?;
    Ok(peer)
}

/// run peers_thread and panic if it exited
fn try_peers_thread(
    poll: Box<dyn Poll>,
    peers: Arc<RwLock<PeerManager>>,
    server_socket: Arc<Socket>,
    passphrase: Option<String>,
) {
    if let Err(error) = peers_thread(poll, peers, server_socket, passphrase) {
        log::error!("peers thread exited with error: {error:?}");
        panic!("peers thread exited")
    }
}

/// thread that handles all incoming packets to each `Peer`
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
        server_socket
            .send_to(buffer, peer.get_client_addr())
            .unwrap();
    });
    poll.poll(peers, on_peer_recv)?;
    Ok(())
}

/// run cleanup thread
fn cleanup_thread(peer_manager: Arc<RwLock<PeerManager>>) {
    loop {
        std::thread::sleep(CLEANUP_INTERVAL);
        try_cleanup(&peer_manager);
    }
}

/// try cleaning peers that has not been used for about `CLEANUP_INTERVAL` duration.
fn try_cleanup(peer_manager: &RwLock<PeerManager>) {
    let mut peers = peer_manager.write();
    let mut used_client_count = 0;
    for peer in peers.get_all() {
        let used = peer.reset_used();
        if !used {
            let client_addr = peer.get_client_addr();
            log::info!("cleaning peer that handled '{client_addr}'");
            if let Err(error) = peers.remove_peer(&peer) {
                log::warn!("couldn't remove peer of '{client_addr}': {error:?}");
            }
        } else {
            used_client_count += 1;
        }
    }
    log::info!("{used_client_count} clients remaining after cleanup");
}
