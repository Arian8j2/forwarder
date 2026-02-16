use super::Poll;
use crate::{
    peer::{Peer, PeerManager},
    socket::icmp::{header_offset, parse_icmp_packet, IcmpEchoType, IcmpSocket, ICMP_HEADER_LEN},
    utils::cast_maybe_uninit,
    MAX_PACKET_SIZE,
};
use parking_lot::RwLock;
use std::{net::SocketAddr, sync::Arc};

#[derive(Debug)]
pub struct IcmpPoll {
    pub remote_addr: SocketAddr,
    pub echo_type: IcmpEchoType,
}

impl Poll for IcmpPoll {
    fn get_registry(&self) -> anyhow::Result<Option<Box<dyn super::Registry>>> {
        Ok(None)
    }

    fn poll(
        &mut self,
        peers: Arc<RwLock<PeerManager>>,
        on_peer_recv: Box<dyn Fn(&Peer, &mut [u8])>,
    ) -> anyhow::Result<()> {
        let is_ipv6 = self.remote_addr.is_ipv6();
        let listen_addr = crate::peer::create_any_addr(is_ipv6);
        let socket: socket2::Socket = IcmpSocket::inner_bind(listen_addr)?;

        #[cfg(target_os = "linux")]
        {
            let filter = crate::socket::icmp::create_bfp_filter(
                is_ipv6,
                self.echo_type,
                self.remote_addr.port(),
            );
            if let Err(error) = socket.attach_filter(&filter) {
                log::warn!("couldn't attach bpf filter: {error:?}");
            }
        }

        let mut buffer = [0u8; MAX_PACKET_SIZE];
        let header_offset = header_offset(is_ipv6);

        loop {
            let Ok(size) = socket.recv(cast_maybe_uninit(&mut buffer)) else {
                continue;
            };
            let Some(icmp_packet) =
                parse_icmp_packet(&buffer[header_offset..size], is_ipv6, self.echo_type)
            else {
                continue;
            };
            if icmp_packet.src_port != self.remote_addr.port() {
                continue;
            }
            let peers = peers.read();
            let Some(peer) = peers.find_peer_with_port(&icmp_packet.dst_port) else {
                continue;
            };
            let payload_offset = header_offset + ICMP_HEADER_LEN;
            on_peer_recv(peer, &mut buffer[payload_offset..size]);
        }
    }
}
