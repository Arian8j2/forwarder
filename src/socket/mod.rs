use async_trait::async_trait;
use std::{io::Result, net::SocketAddr};

#[async_trait]
pub trait Socket: Send + Sync {
    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize>;
    async fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)>;
    async fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> Result<usize>;
    async fn send(&self, buffer: &[u8]) -> Result<usize>;
    async fn connect(&mut self, addr: &SocketAddr) -> Result<()>;
    fn local_addr(&mut self) -> Result<SocketAddr>;
}

mod protocol;
mod uri;
pub(crate) use protocol::SocketProtocol;
pub(crate) use uri::SocketUri;

mod icmp;
mod udp;
