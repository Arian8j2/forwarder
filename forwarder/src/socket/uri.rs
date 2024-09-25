use super::SocketProtocol;
use anyhow::ensure;
use std::{fmt::Display, net::SocketAddr, str::FromStr};

/// # Examples
/// ```
/// use forwarder::socket::{SocketUri, SocketProtocol};
/// use std::{str::FromStr, net::{IpAddr, Ipv4Addr, SocketAddr}};
///
/// let uri = SocketUri::from_str("127.0.0.1:8000/udp")?;
/// assert_eq!(
///     uri.addr,
///     SocketAddr::new(
///         IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
///         8000
///     )
/// );
/// assert_eq!(uri.protocol, SocketProtocol::Udp);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub struct SocketUri {
    pub addr: SocketAddr,
    pub protocol: SocketProtocol,
}

#[allow(unused)]
impl SocketUri {
    pub fn new(addr: SocketAddr, protocol: SocketProtocol) -> Self {
        SocketUri { addr, protocol }
    }
}

impl FromStr for SocketUri {
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
            Some(protocol_str) => SocketProtocol::from_str(protocol_str)?,
            // if protocol is not provided we consider it's udp
            None => SocketProtocol::Udp,
        };

        Ok(SocketUri { addr, protocol })
    }
}

impl Display for SocketUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.addr, self.protocol)
    }
}

impl TryFrom<&str> for SocketUri {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_uri_should_fail() {
        assert!(SocketUri::from_str("127,0:8000/udp").is_err());
        assert!(SocketUri::from_str("127.0.0.1:8000/haha").is_err());
        assert!(SocketUri::from_str("").is_err());
    }
}
