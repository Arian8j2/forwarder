use super::Poll;
use crate::{
    create_socket_buffer,
    peer::{Peer, PeerManager},
    socket::pushack::{parse_pushack_packet, PushackSocket},
    utils::cast_maybe_uninit,
    MAX_PACKET_SIZE,
};
use parking_lot::RwLock;
use smoltcp::wire::{IPV4_HEADER_LEN, TCP_HEADER_LEN};
use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};

#[derive(Debug)]
pub struct PushackPoll {
    pub remote_addr: SocketAddr,
}

impl Poll for PushackPoll {
    fn get_registry(&self) -> anyhow::Result<Option<Box<dyn super::Registry>>> {
        Ok(None)
    }

    fn poll(
        &mut self,
        peers: Arc<RwLock<PeerManager>>,
        on_peer_recv: Box<dyn Fn(&Peer, &mut [u8])>,
    ) -> anyhow::Result<()> {
        let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);
        let socket = PushackSocket::inner_bind(addr.into())?;
        let buffer = create_socket_buffer!(MAX_PACKET_SIZE);

        #[cfg(target_os = "linux")]
        {
            use crate::socket::pushack::{create_bfp_filter, TcpBpfFilter};
            let filter = create_bfp_filter(TcpBpfFilter::SrcPort(self.remote_addr.port()));
            if let Err(error) = socket.attach_filter(&filter) {
                log::warn!("couldn't attach bpf filter: {error:?}");
            }
        }

        loop {
            let Ok(size) = socket.recv(cast_maybe_uninit(buffer)) else {
                continue;
            };
            let Some(tcp_packet) = parse_pushack_packet(&buffer[..size]) else {
                continue;
            };
            if tcp_packet.src_port != self.remote_addr.port() {
                continue;
            }
            let peers = peers.read();
            let Some(peer) = peers.find_peer_with_port(&tcp_packet.dst_port) else {
                continue;
            };
            let payload_offset = IPV4_HEADER_LEN + TCP_HEADER_LEN;
            on_peer_recv(peer, &mut buffer[payload_offset..size]);
        }
    }
}
