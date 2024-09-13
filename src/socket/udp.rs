use super::Socket;
use std::{io::Result, net::SocketAddr};

pub struct UdpSocket(std::net::UdpSocket);

impl UdpSocket {
    pub fn bind(address: &SocketAddr) -> Result<Self> {
        let socket = std::net::UdpSocket::bind(address)?;
        Ok(UdpSocket(socket))
    }
}

impl Socket for UdpSocket {
    fn recv(&mut self, buffer: &mut [u8]) -> Result<usize> {
        self.0.recv(buffer)
    }

    fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.0.recv_from(buffer)
    }

    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> Result<usize> {
        self.0.send_to(buffer, to)
    }

    fn send(&self, buffer: &[u8]) -> Result<usize> {
        self.0.send(buffer)
    }

    fn connect(&mut self, addr: &SocketAddr) -> Result<()> {
        self.0.connect(addr)
    }

    fn local_addr(&mut self) -> Result<SocketAddr> {
        self.0.local_addr()
    }
}
