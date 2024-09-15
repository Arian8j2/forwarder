mod encryption;
mod peer;
pub mod socket;

use anyhow::Context;
use log::info;
use mio::{unix::SourceFd, Events, Interest, Poll, Registry};
use parking_lot::{RwLock, RwLockUpgradableReadGuard, RwLockWriteGuard};
use std::{net::SocketAddr, sync::Arc};
use {
    peer::{Peer, PeerManager},
    socket::{Socket, SocketTrait, SocketUri},
};

const EPOLL_EVENTS_CAPACITY: usize = 1024;
pub const MAX_PACKET_SIZE: usize = 65535;

pub fn run_server(
    listen_uri: SocketUri,
    remote_uri: SocketUri,
    passphrase: Option<String>,
) -> anyhow::Result<()> {
    let listen_addr = &listen_uri.addr;
    let socket = listen_uri
        .protocol
        .bind(&listen_addr)
        .with_context(|| format!("couldn't listen on '{listen_addr}'"))?;
    let socket = Arc::new(socket);
    info!("listen on '{listen_addr}'");

    let poll = Poll::new().with_context(|| "couldn't create epoll")?;
    let registry = poll
        .registry()
        .try_clone()
        .with_context(|| "couldn't copy mio registry")?;

    let peer_manager: Arc<RwLock<PeerManager>> = Arc::new(RwLock::new(PeerManager::new()));
    // spawning peers thread
    {
        let peer_manager = peer_manager.clone();
        let server_socket = socket.clone();
        std::thread::spawn(|| try_peers_thread(poll, peer_manager, server_socket))
    };

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
    let (new_peer, token) = Peer::create(&remote_uri, from_addr)?;
    let peer = peers.add_peer(new_peer);
    registry
        .register(
            &mut SourceFd(&peer.socket.as_raw_fd()),
            token,
            Interest::READABLE,
        )
        .with_context(|| "couldn't add new peer to mio registry")?;
    Ok(peer)
}

/// run peers_thread and panic if it exited
fn try_peers_thread(poll: Poll, peers: Arc<RwLock<PeerManager>>, server_socket: Arc<Socket>) {
    if let Err(error) = peers_thread(poll, peers, server_socket) {
        log::error!("peers thread exited with error: {error:?}");
        panic!("peers thread exited")
    }
}

/// thread that handles all incoming packets to each `Peer`
fn peers_thread(
    mut poll: Poll,
    peers: Arc<RwLock<PeerManager>>,
    server_socket: Arc<Socket>,
) -> anyhow::Result<()> {
    let mut events = Events::with_capacity(EPOLL_EVENTS_CAPACITY);
    let mut buffer = vec![0u8; MAX_PACKET_SIZE];

    loop {
        poll.poll(&mut events, None)?;

        let peers = peers.read();
        for event in &events {
            let token = event.token();
            let peer = peers.find_peer_with_token(&token).unwrap();
            // each epoll event may result in multiple readiness events
            while let Ok((size, _)) = peer.socket.recv_from(&mut buffer) {
                // client <--server socket--- peer <----- remote
                server_socket.send_to(&buffer[..size], peer.get_client_addr())?;
            }
        }
    }
}
