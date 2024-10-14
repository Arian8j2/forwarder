use super::{NonBlockingSocketTrait, SocketTrait};
use std::{
    io,
    net::SocketAddr,
    os::fd::{AsRawFd, RawFd},
};

#[derive(Debug)]
pub struct UdpSocket(std::net::UdpSocket);

impl UdpSocket {
    pub fn bind(address: &SocketAddr) -> io::Result<Self> {
        let socket = std::net::UdpSocket::bind(address)?;
        Ok(UdpSocket(socket))
    }
}

impl SocketTrait for UdpSocket {
    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.0.recv_from(buffer)
    }

    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> io::Result<usize> {
        self.0.send_to(buffer, to)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }
}

#[derive(Debug)]
pub struct NonBlockingUdpSocket(std::net::UdpSocket);

impl NonBlockingUdpSocket {
    pub fn bind(address: &SocketAddr) -> io::Result<Self> {
        let socket = std::net::UdpSocket::bind(address)?;
        socket.set_nonblocking(true)?;
        Ok(Self(socket))
    }

    pub fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl NonBlockingSocketTrait for NonBlockingUdpSocket {
    fn send(&self, buffer: &[u8]) -> io::Result<usize> {
        self.0.send(buffer)
    }

    fn connect(&mut self, addr: &SocketAddr) -> io::Result<()> {
        self.0.connect(addr)
    }

    fn recv(&self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.recv(buffer)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }
}
