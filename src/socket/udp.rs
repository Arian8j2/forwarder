use super::Socket;
use async_trait::async_trait;
use std::{io::Result, net::SocketAddr};

pub struct UdpSocket(tokio::net::UdpSocket);

impl UdpSocket {
    pub async fn bind(address: &SocketAddr) -> Result<Self> {
        let socket = tokio::net::UdpSocket::bind(address).await?;
        Ok(UdpSocket(socket))
    }
}

#[async_trait]
impl Socket for UdpSocket {
    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize> {
        self.0.recv(buffer).await
    }

    async fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.0.recv_from(buffer).await
    }

    async fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> Result<usize> {
        self.0.send_to(buffer, to).await
    }

    async fn send(&self, buffer: &[u8]) -> Result<usize> {
        self.0.send(buffer).await
    }

    async fn connect(&mut self, addr: &SocketAddr) -> Result<()> {
        self.0.connect(addr).await
    }
}
