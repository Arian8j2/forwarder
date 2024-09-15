use enum_dispatch::enum_dispatch;
use std::{io::Result, net::SocketAddr};

#[enum_dispatch]
pub trait SocketTrait {
    fn recv(&self, buffer: &mut [u8]) -> Result<usize>;
    fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)>;
    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> Result<usize>;
    fn send(&self, buffer: &[u8]) -> Result<usize>;
    fn connect(&self, addr: &SocketAddr) -> Result<()>;
    fn local_addr(&self) -> Result<SocketAddr>;
    fn set_nonblocking(&self, nonblocking: bool) -> Result<()>;
    fn as_raw_fd(&self) -> i32;
}

#[derive(Debug)]
#[enum_dispatch(SocketTrait)]
pub enum Socket {
    Udp(udp::UdpSocket),
}

mod protocol;
mod uri;
pub(crate) use protocol::SocketProtocol;
pub use uri::SocketUri;

// mod icmp;
mod udp;
