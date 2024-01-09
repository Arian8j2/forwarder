use super::Socket;
use async_trait::async_trait;
use std::{
    io::{ErrorKind, Result},
    net::{SocketAddr, SocketAddrV4},
};

pub struct UdpSocket(tokio::net::UdpSocket);

impl UdpSocket {
    pub async fn bind(address: &SocketAddrV4) -> Result<Self> {
        let socket = tokio::net::UdpSocket::bind(address).await?;
        Ok(UdpSocket(socket))
    }
}

#[async_trait]
impl Socket for UdpSocket {
    async fn recv(&self, buffer: &mut [u8]) -> Result<usize> {
        self.0.recv(buffer).await
    }

    async fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, SocketAddrV4)> {
        let (len, addr) = self.0.recv_from(buffer).await?;
        match addr {
            SocketAddr::V4(v4_addr) => Ok((len, v4_addr)),
            SocketAddr::V6(_) => Err(ErrorKind::Unsupported.into()),
        }
    }

    async fn send_to(&self, buffer: &[u8], to: &SocketAddrV4) -> Result<usize> {
        self.0.send_to(buffer, to).await
    }

    async fn send(&self, buffer: &[u8]) -> Result<usize> {
        self.0.send(buffer).await
    }

    async fn connect(&self, addr: &SocketAddrV4) -> Result<()> {
        self.0.connect(addr).await
    }
}
