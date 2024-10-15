use crate::{
    peer::{Peer, PeerManager},
    socket::SocketProtocol,
};
use parking_lot::RwLock;
use std::sync::Arc;

mod registry;
pub(crate) use registry::Registry;

type OnPeerRecvCallback = dyn Fn(&Peer, &mut [u8]);

pub trait Poll: Send {
    /// blocks the current thread and listens on multiple registered `NonBlockingSocket`s
    /// at the same time and calls `on_peer_recv` on new packets from peer
    fn poll(
        &mut self,
        peers: Arc<RwLock<PeerManager>>,
        on_peer_recv: Box<OnPeerRecvCallback>,
    ) -> anyhow::Result<()>;

    /// returns clone of poll registry
    fn get_registry(&self) -> anyhow::Result<Box<dyn Registry>>;
}

mod icmp;
mod udp;

pub fn new(protocol: SocketProtocol, is_ipv6: bool) -> anyhow::Result<Box<dyn Poll>> {
    Ok(match protocol {
        SocketProtocol::Udp => Box::new(udp::UdpPoll(mio::Poll::new()?)),
        SocketProtocol::Icmp => Box::new(icmp::IcmpPoll { is_ipv6 }),
    })
}
