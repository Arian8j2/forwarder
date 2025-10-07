use super::{Poll, Registry};
use crate::{
    peer::{Peer, PeerManager},
    socket::{icmp::IcmpSocket, NonBlockingSocket},
    MAX_PACKET_SIZE,
};
use parking_lot::RwLock;
use std::{mem::MaybeUninit, net::SocketAddr, sync::Arc};

#[derive(Debug)]
pub struct IcmpPoll {
    pub remote_addr: SocketAddr,
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
        let is_ipv6 = self.remote_addr.is_ipv6();
        let listen_addr = crate::peer::create_any_addr(is_ipv6);
        let socket: socket2::Socket = IcmpSocket::inner_bind(listen_addr)?;
        let mut buffer = [0u8; MAX_PACKET_SIZE];

        #[cfg(target_os = "linux")]
        if !is_ipv6 {
            let filter = create_bpf_filter(self.remote_addr.port());
            if let Err(error) = socket.attach_filter(&filter) {
                // filter is not required so continue if it errors
                log::warn!("couldn't attach bpf filter to socket: {error:?}");
            }
        }

        loop {
            let Ok(size) =
                socket.recv(unsafe { &mut *(&mut buffer as *mut [u8] as *mut [MaybeUninit<u8>]) })
            else {
                continue;
            };
            let Some(icmp_packet) =
                crate::socket::icmp::parse_icmp_packet(&mut buffer[..size], is_ipv6)
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

#[cfg(target_os = "linux")]
fn create_bpf_filter(remote_port: u16) -> [libc::sock_filter; 4] {
    [
        (0x28, 0, 0, 0x0000001a),         // ldh [26]          ; icmp sequence
        (0x15, 0, 1, remote_port as u32), // jne #port, drop
        (0x06, 0, 0, 0xffffffff),         // ret #-1
        (0x06, 0, 0, 0000000000),         // drop: ret #0
    ]
    .map(|(code, jt, jf, k)| libc::sock_filter { code, jt, jf, k })
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
