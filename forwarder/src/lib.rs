mod encryption;
mod peer;
pub mod socket;

use anyhow::Context;
use log::info;
use mio::{Events, Poll, Registry};
use parking_lot::{RwLock, RwLockUpgradableReadGuard, RwLockWriteGuard};
use std::{
    net::SocketAddr,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use {
    peer::{Peer, PeerManager},
    socket::{Socket, SocketTrait, SocketUri},
};

const EPOLL_EVENTS_CAPACITY: usize = 1024;
pub const MAX_PACKET_SIZE: usize = 65535;
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
    info!("listen on '{listen_addr}'");

    let poll = Poll::new().expect("cannot create epoll");
    let registry = poll
        .registry()
        .try_clone()
        .expect("couldn't copy mio registry");

    let peer_manager: Arc<RwLock<PeerManager>> = Arc::new(RwLock::new(PeerManager::new()));
    {
        let peer_manager = peer_manager.clone();
        let server_socket = socket.clone();
        let passphrase = passphrase.clone();
        std::thread::spawn(|| try_peers_thread(poll, peer_manager, server_socket, passphrase))
    };
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
                peer.used.store(true, Ordering::Relaxed);
                // client ---> server socket ---peer socket----> remote
                peer.socket.send(&buffer[..size]).ok();
            }
            None => {
                log::info!("new client '{from_addr}'");
                let peers = RwLockUpgradableReadGuard::upgrade(peers);
                let peer = match add_new_peer(&remote_uri, from_addr, peers, &registry) {
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
    registry: &Registry,
) -> anyhow::Result<Arc<Peer>> {
    let (mut new_peer, token) = Peer::create(remote_uri, from_addr)?;
    new_peer
        .socket
        .register(registry, token)
        .with_context(|| "couldn't add new peer to mio registry")?;
    let peer = peers.add_peer(new_peer);
    Ok(peer)
}

/// run peers_thread and panic if it exited
fn try_peers_thread(
    poll: Poll,
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
    mut poll: Poll,
    peers: Arc<RwLock<PeerManager>>,
    server_socket: Arc<Socket>,
    passphrase: Option<String>,
) -> anyhow::Result<()> {
    let mut events = Events::with_capacity(EPOLL_EVENTS_CAPACITY);
    let mut buffer = vec![0u8; MAX_PACKET_SIZE];

    loop {
        poll.poll(&mut events, None)?;

        let peers = peers.read();
        for event in &events {
            let token = event.token();
            let peer = peers.find_peer_with_token(&token).unwrap();
            peer.used.store(true, Ordering::Relaxed);
            // each epoll event may result in multiple readiness events
            while let Ok(size) = peer.socket.recv(&mut buffer) {
                if let Some(ref passphrase) = passphrase {
                    encryption::xor_encrypt(&mut buffer[..size], passphrase)
                }
                // client <--server socket--- peer <----- remote
                server_socket.send_to(&buffer[..size], peer.get_client_addr())?;
            }
        }
    }
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
        let used = peer.used.swap(false, Ordering::Relaxed);
        if !used {
            let client_addr = peer.get_client_addr();
            log::info!("cleaning peer that handled '{client_addr}'");
            peers.remove_peer(client_addr, peer.get_token());
        } else {
            used_client_count += 1;
        }
    }
    log::info!("{used_client_count} clients remaining after cleanup");
}
