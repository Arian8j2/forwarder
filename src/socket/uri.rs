use super::SocketProtocol;
use std::{net::SocketAddrV4, str::FromStr};

#[derive(Clone, Copy, Debug)]
pub struct SocketUri {
    pub addr: SocketAddrV4,
    pub protocol: SocketProtocol,
}

#[allow(unused)]
impl SocketUri {
    pub fn new(addr: SocketAddrV4, protocol: SocketProtocol) -> Self {
        SocketUri { addr, protocol }
    }
}

impl FromStr for SocketUri {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split("/").collect();
        if parts.len() > 2 {
            return Err(
                "There are more parts than two, You can only use slash once in uri".to_owned(),
            );
        }

        let addr_str = parts
            .first()
            .ok_or("Uri need to have address part like '127.0.0.1'")?;
        let addr = SocketAddrV4::from_str(&addr_str).map_err(|e| e.to_string())?;

        let protocol = match parts.get(1) {
            Some(protocol_str) => SocketProtocol::from_str(protocol_str)?,
            None => SocketProtocol::Udp,
        };
        Ok(SocketUri { addr, protocol })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_udp_uri() {
        let input = "127.0.0.1:8000/udp";
        let uri = SocketUri::from_str(input).unwrap();
        assert_eq!(
            uri.addr,
            SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8000)
        );
        assert_eq!(uri.protocol, SocketProtocol::Udp);
    }

    #[test]
    fn test_invalid_uri_should_fail() {
        assert!(SocketUri::from_str("127,0:8000/udp").is_err());
        assert!(SocketUri::from_str("127.0.0.1:8000/haha").is_err());
        assert!(SocketUri::from_str("").is_err());
    }
}
