use async_trait::async_trait;
use std::{io::Result, net::SocketAddr, str::FromStr};

#[async_trait]
pub trait Socket: Send + Sync {
    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize>;
    async fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)>;
    async fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> Result<usize>;
    async fn send(&self, buffer: &[u8]) -> Result<usize>;
    async fn connect(&mut self, addr: &SocketAddr) -> Result<()>;
}

mod udp;
use udp::UdpSocket;

mod icmp;
pub(crate) use icmp::setting::IcmpSettingSetter;
use icmp::IcmpSocket;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SocketProtocol {
    Udp,
    Icmp,
}

impl SocketProtocol {
    pub async fn bind(self, addr: &SocketAddr) -> Result<Box<dyn Socket>> {
        let socket: Box<dyn Socket> = match self {
            SocketProtocol::Udp => Box::new(UdpSocket::bind(addr).await?),
            SocketProtocol::Icmp => {
                let SocketAddr::V4(v4_addr) = addr else {
                    unimplemented!("icmp socket doesn't support ipv6")
                };
                Box::new(IcmpSocket::bind(v4_addr).await?)
            }
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
