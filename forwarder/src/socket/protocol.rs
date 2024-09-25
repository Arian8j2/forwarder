use anyhow::bail;
use std::{fmt::Display, str::FromStr};

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SocketProtocol {
    Udp,
    Icmp,
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
