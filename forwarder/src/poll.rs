use crate::{
    peer::{Peer, PeerManager},
    socket::NonBlockingSocket,
    uri::Protocol,
};
use parking_lot::RwLock;
use std::sync::Arc;

type OnPeerRecvCallback = dyn Fn(&Peer, &mut [u8]);

/// trait to be able to listen on multiple sockets asynchronously
pub trait Poll: Send {
    /// blocks the current thread and listens on multiple registered `NonBlockingSocket`'s
    /// at the same time and calls `on_peer_recv` on new packets from peer
    fn poll(
        &mut self,
        peers: Arc<RwLock<PeerManager>>,
        on_peer_recv: Box<OnPeerRecvCallback>,
    ) -> anyhow::Result<()>;

    /// returns clone of poll registry
    fn get_registry(&self) -> anyhow::Result<Box<dyn Registry>>;
}

/// trait that allows others to register socket to `Poll`
pub trait Registry: Send + Sync {
    // need Sync because parking_lot::RwLock needs inner to be Sync
    fn register(&self, socket: &mut NonBlockingSocket) -> anyhow::Result<()>;
    fn deregister(&self, socket: &mut NonBlockingSocket) -> anyhow::Result<()>;
}

mod icmp;
mod udp;

pub fn new(protocol: Protocol, is_ipv6: bool) -> anyhow::Result<Box<dyn Poll>> {
    Ok(match protocol {
        Protocol::Udp => Box::new(udp::UdpPoll(mio::Poll::new()?)),
        Protocol::Icmp => Box::new(icmp::IcmpPoll { is_ipv6 }),
    })
}
