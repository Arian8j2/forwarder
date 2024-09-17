use super::SocketTrait;
use mio::{unix::SourceFd, Interest};
use std::{io, net::SocketAddr, os::fd::AsRawFd};

#[derive(Debug)]
pub struct UdpSocket(std::net::UdpSocket);

impl UdpSocket {
    pub fn bind(address: &SocketAddr) -> io::Result<Self> {
        let socket = std::net::UdpSocket::bind(address)?;
        Ok(UdpSocket(socket))
    }
}

impl SocketTrait for UdpSocket {
    fn recv(&self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buffer)
    }

    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.0.recv_from(buffer)
    }

    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> io::Result<usize> {
        self.0.send_to(buffer, to)
    }

    fn send(&self, buffer: &[u8]) -> io::Result<usize> {
        self.0.send(buffer)
    }

    fn connect(&mut self, addr: &SocketAddr) -> io::Result<()> {
        self.0.connect(addr)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        self.0.set_nonblocking(nonblocking)
    }

    fn unique_token(&self) -> mio::Token {
        mio::Token(self.0.as_raw_fd() as usize)
    }

    fn register(&mut self, registry: &mio::Registry, token: mio::Token) -> io::Result<()> {
        registry.register(
            &mut SourceFd(&self.0.as_raw_fd()),
            token,
            Interest::READABLE,
        )
    }
}
