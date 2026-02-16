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
use std::{net::SocketAddr, sync::Arc};

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
        let socket = PushackSocket::bind(&"0.0.0.0:0".parse().unwrap())?;
        let buffer = create_socket_buffer!(MAX_PACKET_SIZE);

        loop {
            let Ok(size) = socket.inner_socket().recv(cast_maybe_uninit(buffer)) else {
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
