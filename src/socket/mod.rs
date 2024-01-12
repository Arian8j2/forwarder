use async_trait::async_trait;
use std::{io::Result, net::SocketAddrV4, str::FromStr};

#[async_trait]
pub trait Socket: Send + Sync {
    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize>;
    async fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddrV4)>;
    async fn send_to(&self, buffer: &[u8], to: &SocketAddrV4) -> Result<usize>;
    async fn send(&self, buffer: &[u8]) -> Result<usize>;
    async fn connect(&mut self, addr: &SocketAddrV4) -> Result<()>;
}

mod udp;
use udp::UdpSocket;

mod icmp;
use icmp::IcmpSocket;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SocketProtocol {
    Udp,
    Icmp,
}

impl SocketProtocol {
    pub async fn bind(self, addr: &SocketAddrV4) -> Result<Box<dyn Socket>> {
        let socket: Box<dyn Socket> = match self {
            SocketProtocol::Udp => Box::new(UdpSocket::bind(addr).await?),
            SocketProtocol::Icmp => Box::new(IcmpSocket::bind(addr).await?),
        };
        Ok(socket)
    }
}

impl FromStr for SocketProtocol {
    type Err = &'static str;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "udp" => Ok(SocketProtocol::Udp),
            "icmp" => Ok(SocketProtocol::Icmp),
            _ => Err("Invalid socket protocl name, valid socket protocols are: 'udp'"),
        }
    }
}

mod uri;
pub(crate) use uri::SocketUri;
