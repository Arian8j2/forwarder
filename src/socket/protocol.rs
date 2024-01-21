use super::{icmp::IcmpSocket, udp::UdpSocket, Socket};
use std::{io::Result, net::SocketAddr, str::FromStr};

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
