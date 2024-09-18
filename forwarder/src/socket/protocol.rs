use anyhow::bail;

use super::{icmp::IcmpSocket, udp::UdpSocket, Socket};
use std::{fmt::Display, io::Result, net::SocketAddr, str::FromStr};

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SocketProtocol {
    Udp,
    Icmp,
}

impl SocketProtocol {
    pub fn bind(self, addr: &SocketAddr) -> Result<Socket> {
        let socket = match self {
            SocketProtocol::Udp => Socket::Udp(UdpSocket::bind(addr)?),
            SocketProtocol::Icmp => Socket::Icmp(IcmpSocket::bind(addr)?),
        };
        Ok(socket)
    }
}

impl FromStr for SocketProtocol {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_lowercase().as_str() {
            "udp" => Ok(SocketProtocol::Udp),
            "icmp" => Ok(SocketProtocol::Icmp),
            _ => {
                bail!("invalid socket protocol name, valid socket protocols are: 'udp' and 'icmp'")
            }
        }
    }
}

impl Display for SocketProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            SocketProtocol::Icmp => "icmp".to_owned(),
            SocketProtocol::Udp => "udp".to_owned(),
        };
        write!(f, "{str}")
    }
}
