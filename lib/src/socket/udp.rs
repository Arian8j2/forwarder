use super::SocketTrait;
use std::{io::Result, net::SocketAddr, os::fd::AsRawFd};

#[derive(Debug)]
pub struct UdpSocket(std::net::UdpSocket);

impl UdpSocket {
    pub fn bind(address: &SocketAddr) -> Result<Self> {
        let socket = std::net::UdpSocket::bind(address)?;
        Ok(UdpSocket(socket))
    }
}

impl SocketTrait for UdpSocket {
    fn recv(&self, buffer: &mut [u8]) -> Result<usize> {
        self.0.recv(buffer)
    }

    fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.0.recv_from(buffer)
    }

    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> Result<usize> {
        self.0.send_to(buffer, to)
    }

    fn send(&self, buffer: &[u8]) -> Result<usize> {
        self.0.send(buffer)
    }

    fn connect(&self, addr: &SocketAddr) -> Result<()> {
        self.0.connect(addr)
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        self.0.local_addr()
    }

    fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
        self.0.set_nonblocking(nonblocking)
    }

    fn as_raw_fd(&self) -> i32 {
        self.0.as_raw_fd()
    }
}
