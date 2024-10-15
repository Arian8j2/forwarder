use super::{
    registry::{IcmpRegistry, Registry},
    Poll,
};
use crate::{
    peer::{Peer, PeerManager},
    socket::icmp::IcmpSocket,
    MAX_PACKET_SIZE,
};
use parking_lot::RwLock;
use std::{mem::MaybeUninit, sync::Arc};

#[derive(Debug)]
pub struct IcmpPoll {
    pub is_ipv6: bool,
}

impl Poll for IcmpPoll {
    fn get_registry(&self) -> anyhow::Result<Box<dyn Registry>> {
        Ok(Box::new(IcmpRegistry))
    }

    fn poll(
        &mut self,
        peers: Arc<RwLock<PeerManager>>,
        on_peer_recv: Box<dyn Fn(&Peer, &mut [u8])>,
    ) -> anyhow::Result<()> {
        let socket: socket2::Socket = IcmpSocket::inner_bind("127.0.0.1:0".parse()?)?;
        let mut buffer = [0u8; MAX_PACKET_SIZE];

        loop {
            let Ok(size) =
                socket.recv(unsafe { &mut *(&mut buffer as *mut [u8] as *mut [MaybeUninit<u8>]) })
            else {
                continue;
            };
            let Some(icmp_packet) =
                crate::socket::icmp::parse_icmp_packet(&mut buffer[..size], self.is_ipv6)
            else {
                continue;
            };
            let peers = peers.read();
            let port = icmp_packet.dst_port;
            let Some(peer) = peers.find_peer_with_port(&port) else {
                continue;
            };
            on_peer_recv(peer, icmp_packet.payload);
        }
    }
}