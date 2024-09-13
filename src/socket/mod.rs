use std::{io::Result, net::SocketAddr};

pub trait Socket {
    fn recv(&mut self, buffer: &mut [u8]) -> Result<usize>;
    fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)>;
    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> Result<usize>;
    fn send(&self, buffer: &[u8]) -> Result<usize>;
    fn connect(&mut self, addr: &SocketAddr) -> Result<()>;
    fn local_addr(&mut self) -> Result<SocketAddr>;
}

mod protocol;
mod uri;
pub(crate) use protocol::SocketProtocol;
pub(crate) use uri::SocketUri;

// mod icmp;
mod udp;
