use super::{
    registry::{Registry, UdpRegistry},
    Poll,
};
use crate::{
    peer::{Peer, PeerManager},
    MAX_PACKET_SIZE,
};
use mio::Events;
use parking_lot::RwLock;
use std::sync::Arc;

const EPOLL_EVENTS_CAPACITY: usize = 1024;

#[derive(Debug)]
pub struct UdpPoll(pub mio::Poll);

impl Poll for UdpPoll {
    fn get_registry(&self) -> anyhow::Result<Box<dyn Registry>> {
        let registry = self.0.registry().try_clone()?;
        let registry = UdpRegistry(registry);
        Ok(Box::new(registry))
    }

    fn poll(
        &mut self,
        peers: Arc<RwLock<PeerManager>>,
        on_peer_recv: Box<dyn Fn(&Peer, &mut [u8])>,
    ) -> anyhow::Result<()> {
        let mut events = Events::with_capacity(EPOLL_EVENTS_CAPACITY);
        let mut buffer = vec![0u8; MAX_PACKET_SIZE];

        loop {
            self.0.poll(&mut events, None)?;

            let peers = peers.read();
            for event in &events {
                let port = event.token().0 as u16;
                let Some(peer) = peers.find_peer_with_port(&port) else {
                    continue;
                };
                peer.set_used();
                // each epoll event may result in multiple readiness events
                while let Ok(size) = peer.socket.recv(&mut buffer) {
                    on_peer_recv(peer, &mut buffer[..size]);
                }
            }
        }
    }
}
