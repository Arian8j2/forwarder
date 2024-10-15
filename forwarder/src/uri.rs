use anyhow::{bail, ensure};
use std::{fmt::Display, net::SocketAddr, str::FromStr};

/// # Examples
/// ```
/// use forwarder::uri::{Uri, Protocol};
/// use std::{str::FromStr, net::{IpAddr, Ipv4Addr, SocketAddr}};
///
/// let uri = Uri::from_str("127.0.0.1:8000/udp")?;
/// assert_eq!(
///     uri.addr,
///     SocketAddr::new(
///         IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
///         8000
///     )
/// );
/// assert_eq!(uri.protocol, Protocol::Udp);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Uri {
    pub addr: SocketAddr,
    pub protocol: Protocol,
}

#[allow(unused)]
impl Uri {
    pub fn new(addr: SocketAddr, protocol: Protocol) -> Self {
        Uri { addr, protocol }
    }
}

impl FromStr for Uri {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        let parts: Vec<&str> = s.split('/').collect();
        ensure!(
            parts.len() <= 2,
            "there are more parts than two, you can only use slash once in uri"
        );

        let addr_str = parts.first().ok_or(anyhow::anyhow!(
            "uri need to have address part like '127.0.0.1:8080'"
        ))?;
        let addr = SocketAddr::from_str(addr_str)?;

        let protocol = match parts.get(1) {
            Some(protocol_str) => Protocol::from_str(protocol_str)?,
            // if protocol is not provided we consider it's udp
            None => Protocol::Udp,
        };

        Ok(Uri { addr, protocol })
    }
}

impl Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.addr, self.protocol)
    }
}

impl TryFrom<&str> for Uri {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Protocol {
    Udp,
    Icmp,
}

impl FromStr for Protocol {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_lowercase().as_str() {
            "udp" => Ok(Protocol::Udp),
            "icmp" => Ok(Protocol::Icmp),
            _ => {
                bail!("invalid socket protocol name, valid socket protocols are: 'udp' and 'icmp'")
            }
        }
    }
}

impl Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Protocol::Icmp => "icmp".to_owned(),
            Protocol::Udp => "udp".to_owned(),
        };
        write!(f, "{str}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_uri_should_fail() {
        assert!(Uri::from_str("127,0:8000/udp").is_err());
        assert!(Uri::from_str("127.0.0.1:8000/haha").is_err());
        assert!(Uri::from_str("").is_err());
    }
}
