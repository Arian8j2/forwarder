// use super::icmp::IcmpSocket;
use super::{udp::UdpSocket, Socket};
use std::{fmt::Display, io::Result, net::SocketAddr, str::FromStr};

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SocketProtocol {
    Udp,
    // Icmp,
}

impl SocketProtocol {
    pub fn bind(self, addr: &SocketAddr) -> Result<Socket> {
        let socket = match self {
            SocketProtocol::Udp => Socket::Udp(UdpSocket::bind(addr)?),
            // SocketProtocol::Icmp => Box::new(IcmpSocket::bind(addr).await?),
        };
        Ok(socket)
    }
}

impl FromStr for SocketProtocol {
    type Err = &'static str;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "udp" => Ok(SocketProtocol::Udp),
            // "icmp" => Ok(SocketProtocol::Icmp),
            _ => Err("Invalid socket protocl name, valid socket protocols are: 'udp' and 'icmp'"),
        }
    }
}

impl Display for SocketProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            // SocketProtocol::Icmp => "icmp".to_owned(),
            SocketProtocol::Udp => "udp".to_owned(),
        };
        write!(f, "{str}")
    }
}
