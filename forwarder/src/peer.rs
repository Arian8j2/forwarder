use crate::poll::Registry;
use crate::socket::NonBlockingSocket;
use crate::uri::Uri;
use std::fmt::Debug;
use std::{
    borrow::Borrow,
    collections::BTreeMap,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    sync::atomic::Ordering,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Debug)]
pub struct Peer {
    pub socket: NonBlockingSocket,
    client_addr: SocketAddr,
    used: AtomicBool,
}

impl Peer {
    pub fn new(remote_uri: &Uri, client_addr: SocketAddr) -> anyhow::Result<Self> {
        let addr = create_any_addr(remote_uri.addr.is_ipv6());
        let mut socket = NonBlockingSocket::bind(remote_uri.protocol, &addr)?;
        socket.connect(&remote_uri.addr)?;
        let peer = Self {
            socket,
            client_addr,
            used: AtomicBool::new(true),
        };
        Ok(peer)
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
}

pub fn create_any_addr(is_ipv6: bool) -> SocketAddr {
    if is_ipv6 {
        SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into()
    } else {
        SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into()
    }
}

// we keep multiple `BTreeMap`s because on server we
// need to find peer based on `client_addr` and on peer
// side we need to find peer based on `token` so the
// fastest way (i think) is to have multiple maps that
// points to same `Peer`
pub struct PeerManager {
    client_addr_to_peers: BTreeMap<SocketAddr, Arc<Peer>>,
    port_to_peers: BTreeMap<u16, Arc<Peer>>,
    registry: Box<dyn Registry>,
}

impl PeerManager {
    pub fn new(registry: Box<dyn Registry>) -> Self {
        Self {
            client_addr_to_peers: BTreeMap::new(),
            port_to_peers: BTreeMap::new(),
            registry,
        }
    }

    pub fn add_peer(&mut self, mut new_peer: Peer) -> anyhow::Result<Arc<Peer>> {
        let client_addr = new_peer.client_addr;
        self.registry.register(&mut new_peer.socket)?;
        let peer = Arc::new(new_peer);
        self.client_addr_to_peers.insert(client_addr, peer.clone());
        let peer_port = peer.socket.local_addr()?.port();
        self.port_to_peers.insert(peer_port, peer.clone());
        Ok(peer)
    }

    pub fn find_peer_with_client_addr(&self, addr: &SocketAddr) -> Option<&Peer> {
        self.client_addr_to_peers
            .get(addr)
            .map(|peer| peer.borrow())
    }

    pub fn find_peer_with_port(&self, port: &u16) -> Option<&Peer> {
        self.port_to_peers.get(port).map(|peer| peer.borrow())
    }

    pub fn get_all(&self) -> Vec<Arc<Peer>> {
        self.client_addr_to_peers.values().cloned().collect()
    }

    pub fn remove_peer(&mut self, peer: Arc<Peer>) -> anyhow::Result<()> {
        self.client_addr_to_peers.remove(&peer.client_addr);
        self.port_to_peers.remove(&peer.socket.local_addr()?.port());

        let mut peer =
            Arc::try_unwrap(peer).map_err(|_| anyhow::anyhow!("can't unwrap Arc<peer>"))?;
        self.registry.deregister(&mut peer.socket)?;
        Ok(())
    }
}
