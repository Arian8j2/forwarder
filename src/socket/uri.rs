use super::SocketVariant;
use std::{net::SocketAddrV4, str::FromStr};

#[derive(Clone, Copy, Debug)]
pub struct SocketUri {
    pub addr: SocketAddrV4,
    pub variant: SocketVariant,
}

#[allow(unused)]
impl SocketUri {
    pub fn new(addr: SocketAddrV4, variant: SocketVariant) -> Self {
        SocketUri { addr, variant }
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

        let variant = match parts.get(1) {
            Some(variant_str) => SocketVariant::from_str(variant_str)?,
            None => SocketVariant::Udp,
        };
        Ok(SocketUri { addr, variant })
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
        assert_eq!(uri.variant, SocketVariant::Udp);
    }

    #[test]
    fn test_invalid_uri_should_fail() {
        assert!(SocketUri::from_str("127,0:8000/udp").is_err());
        assert!(SocketUri::from_str("127.0.0.1:8000/haha").is_err());
        assert!(SocketUri::from_str("").is_err());
    }
}
