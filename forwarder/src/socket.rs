use std::{
    io,
    net::SocketAddr,
    ops::{Deref, DerefMut},
};

pub trait SocketTrait {
    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> io::Result<usize>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
}

#[derive(Debug)]
pub enum Socket {
    Udp(udp::UdpSocket),
    Icmp(icmp::IcmpSocket),
}

impl Socket {
    /// creates a socket based on `protocol` and binds it to `addr` address
    pub fn bind(protocol: SocketProtocol, addr: &SocketAddr) -> io::Result<Self> {
        let socket = match protocol {
            SocketProtocol::Udp => Socket::Udp(udp::UdpSocket::bind(addr)?),
            SocketProtocol::Icmp => Socket::Icmp(icmp::IcmpSocket::bind(addr)?),
        };
        Ok(socket)
    }
}

pub trait NonBlockingSocketTrait {
    fn connect(&mut self, addr: &SocketAddr) -> io::Result<()>;
    fn send(&self, buffer: &[u8]) -> io::Result<usize>;
    fn recv(&self, buffer: &mut [u8]) -> io::Result<usize>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
}

#[derive(Debug)]
pub enum NonBlockingSocket {
    Udp(udp::NonBlockingUdpSocket),
    Icmp(icmp::NonBlockingIcmpSocket),
}

impl NonBlockingSocket {
    /// creates a socket based on `protocol` and binds it to `addr` address
    pub fn bind(protocol: SocketProtocol, addr: &SocketAddr) -> io::Result<Self> {
        let socket = match protocol {
            SocketProtocol::Udp => Self::Udp(udp::NonBlockingUdpSocket::bind(addr)?),
            SocketProtocol::Icmp => Self::Icmp(icmp::NonBlockingIcmpSocket::bind(addr)?),
        };
        Ok(socket)
    }

    pub fn as_udp(&self) -> Option<&udp::NonBlockingUdpSocket> {
        match self {
            Self::Udp(inner) => Some(inner),
            _ => None,
        }
    }
}

macro_rules! impl_enum_deref {
    ($enum:ty, $trait:ty) => {
        impl Deref for $enum {
            type Target = $trait;
            fn deref(&self) -> &Self::Target {
                match self {
                    Self::Udp(inner) => inner,
                    Self::Icmp(inner) => inner,
                }
            }
        }
        impl DerefMut for $enum {
            fn deref_mut(&mut self) -> &mut Self::Target {
                match self {
                    Self::Udp(inner) => inner,
                    Self::Icmp(inner) => inner,
                }
            }
        }
    };
}

impl_enum_deref! { NonBlockingSocket, dyn NonBlockingSocketTrait }
impl_enum_deref! { Socket, dyn SocketTrait }

mod protocol;
mod uri;
pub use protocol::SocketProtocol;
pub use uri::SocketUri;

pub(crate) mod icmp;
pub(crate) mod udp;
