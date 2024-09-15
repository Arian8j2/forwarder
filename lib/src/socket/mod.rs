use enum_dispatch::enum_dispatch;
use std::{io, net::SocketAddr};

#[enum_dispatch]
pub trait SocketTrait {
    fn recv(&self, buffer: &mut [u8]) -> io::Result<usize>;
    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> io::Result<usize>;
    fn send(&self, buffer: &[u8]) -> io::Result<usize>;
    fn connect(&self, addr: &SocketAddr) -> io::Result<()>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()>;
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
