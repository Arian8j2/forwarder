use crate::socket::{Socket, SocketTrait, SocketUri};
use mio::Token;
use std::{
    borrow::Borrow,
    collections::BTreeMap,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    sync::atomic::Ordering,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Debug)]
pub struct Peer {
    pub socket: Socket,
    client_addr: SocketAddr,
    token: Token,
    used: AtomicBool,
}

impl Peer {
    pub fn create(
        remote_uri: &SocketUri,
        client_addr: SocketAddr,
    ) -> anyhow::Result<(Self, Token)> {
        let addr: SocketAddr = match remote_uri.addr {
            SocketAddr::V4(_) => SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0).into(),
            SocketAddr::V6(_) => {
                SocketAddrV6::new(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0), 0, 0, 0).into()
            }
        };
        let mut socket = Socket::bind(remote_uri.protocol, &addr)?;
        socket.connect(&remote_uri.addr)?;
        socket.set_nonblocking(true)?;
        let token = socket.unique_token();
        let peer = Self {
            socket,
            token,
            client_addr,
            used: AtomicBool::new(true),
        };
        Ok((peer, token))
    }

    /// mark `Peer` as being used to prevent cleanup thread from cleaning it
    pub fn set_used(&self) {
        self.used
            .compare_exchange_weak(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .ok();
    }

    /// mark `Peer` as not being in use and returns `true` if it was used
    /// before reseting otherwise returns `false`
    pub fn reset_used(&self) -> bool {
        self.used.swap(false, Ordering::Relaxed)
    }

    pub fn get_client_addr(&self) -> &SocketAddr {
        &self.client_addr
    }

    pub fn get_token(&self) -> &Token {
        &self.token
    }
}

// we keep multiple `BTreeMap`s because on server we
// need to find peer based on `client_addr` and on peer
// side we need to find peer based on `token` so the
// fastest way (i think) is to have multiple maps that
// points to same `Peer`
#[derive(Debug)]
pub struct PeerManager {
    client_addr_to_peers: BTreeMap<SocketAddr, Arc<Peer>>,
    token_to_peers: BTreeMap<Token, Arc<Peer>>,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            client_addr_to_peers: BTreeMap::new(),
            token_to_peers: BTreeMap::new(),
        }
    }

    pub fn add_peer(&mut self, new_peer: Peer) -> Arc<Peer> {
        let token = new_peer.token;
        let client_addr = new_peer.client_addr;
        let peer = Arc::new(new_peer);
        self.client_addr_to_peers.insert(client_addr, peer.clone());
        self.token_to_peers.insert(token, peer.clone());
        peer
    }

    pub fn find_peer_with_client_addr(&self, addr: &SocketAddr) -> Option<&Peer> {
        self.client_addr_to_peers
            .get(addr)
            .map(|peer| peer.borrow())
    }

    pub fn find_peer_with_token(&self, token: &Token) -> Option<&Peer> {
        self.token_to_peers.get(token).map(|peer| peer.borrow())
    }

    pub fn get_all(&self) -> Vec<Arc<Peer>> {
        self.client_addr_to_peers.values().cloned().collect()
    }

    pub fn remove_peer(&mut self, client_addr: &SocketAddr, token: &Token) {
        self.client_addr_to_peers.remove(client_addr);
        self.token_to_peers.remove(token);
    }
}
