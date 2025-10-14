use super::{Poll, Registry};
use crate::{
    peer::{Peer, PeerManager},
    socket::{
        icmp::{cast_maybe_uninit, header_offset, parse_icmp_packet, IcmpSocket, ICMP_HEADER_LEN},
        NonBlockingSocket,
    },
    MAX_PACKET_SIZE,
};
use parking_lot::RwLock;
use std::sync::Arc;

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
        let listen_addr = crate::peer::create_any_addr(self.is_ipv6);
        let socket: socket2::Socket = IcmpSocket::inner_bind(listen_addr)?;
        let mut buffer = [0u8; MAX_PACKET_SIZE];
        let header_offset = header_offset(self.is_ipv6);

        loop {
            let Ok(size) = socket.recv(cast_maybe_uninit(&mut buffer)) else {
                continue;
            };
            let Some(icmp_packet) = parse_icmp_packet(&buffer[header_offset..size], self.is_ipv6)
            else {
                continue;
            };
            let peers = peers.read();
            let Some(peer) = peers.find_peer_with_port(&icmp_packet.dst_port) else {
                continue;
            };
            let payload_offset = header_offset + ICMP_HEADER_LEN;
            on_peer_recv(peer, &mut buffer[payload_offset..size]);
        }
    }
}

#[derive(Debug)]
pub struct IcmpRegistry;
// icmp doesn't need a registry because we manage it's poll ourself
impl Registry for IcmpRegistry {
    fn register(&self, _socket: &mut NonBlockingSocket) -> anyhow::Result<()> {
        Ok(())
    }
    fn deregister(&self, _socket: &mut NonBlockingSocket) -> anyhow::Result<()> {
        Ok(())
    }
}
