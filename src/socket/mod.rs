use async_trait::async_trait;
use std::{io::Result, net::SocketAddrV4, str::FromStr};

#[async_trait]
pub trait Socket: Send + Sync {
    async fn recv(&self, buffer: &mut [u8]) -> Result<usize>;
    async fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, SocketAddrV4)>;
    async fn send_to(&self, buffer: &[u8], to: &SocketAddrV4) -> Result<usize>;
    async fn send(&self, buffer: &[u8]) -> Result<usize>;
    async fn connect(&self, addr: &SocketAddrV4) -> Result<()>;
}

mod udp;
pub(crate) use udp::UdpSocket;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SocketVariant {
    Udp,
}

impl SocketVariant {
    pub async fn bind(self, addr: &SocketAddrV4) -> Result<Box<dyn Socket>> {
        let socket: Box<dyn Socket> = match self {
            SocketVariant::Udp => Box::new(UdpSocket::bind(addr).await?),
        };
        Ok(socket)
    }
}

impl FromStr for SocketVariant {
    type Err = &'static str;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "udp" => Ok(SocketVariant::Udp),
            _ => Err("Invalid socket variant name, valid socket variants are: 'udp'"),
        }
    }
}

mod uri;
pub(crate) use uri::SocketUri;
