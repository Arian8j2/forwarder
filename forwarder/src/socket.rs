use crate::{socket::icmp::IcmpEchoType, uri::Protocol};
use std::{
    io,
    net::SocketAddr,
    ops::{Deref, DerefMut},
};

macro_rules! impl_enum_deref {
    ($enum:ty, $trait:ty) => {
        impl Deref for $enum {
            type Target = $trait;
            fn deref(&self) -> &Self::Target {
                match self {
                    Self::Udp(inner) => inner,
                    Self::Icmp(inner) => inner,
                    Self::Pushack(inner) => inner,
                }
            }
        }
        impl DerefMut for $enum {
            fn deref_mut(&mut self) -> &mut Self::Target {
                match self {
                    Self::Udp(inner) => inner,
                    Self::Icmp(inner) => inner,
                    Self::Pushack(inner) => inner,
                }
            }
        }
    };
}

// using enum instead of vtable because i think it's more performant
#[derive(Debug)]
pub enum Socket {
    Udp(udp::UdpSocket),
    Icmp(icmp::IcmpSocket),
    Pushack(pushack::PushackSocket),
}

impl Socket {
    /// creates a socket based on `protocol` and binds it to `addr` address
    pub fn bind(
        protocol: Protocol,
        addr: &SocketAddr,
        icmp_type: IcmpEchoType,
    ) -> io::Result<Self> {
        let socket = match protocol {
            Protocol::Udp => Socket::Udp(udp::UdpSocket::bind(addr)?),
            Protocol::Icmp => Socket::Icmp(icmp::IcmpSocket::bind(addr, icmp_type, true)?),
            Protocol::Pushack => Socket::Pushack(pushack::PushackSocket::bind(addr)?),
        };
        Ok(socket)
    }
}

pub trait SocketTrait {
    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
    fn send_to(&self, buffer: &mut [u8], to: &SocketAddr) -> io::Result<usize>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
}
impl_enum_deref! { Socket, dyn SocketTrait }

#[derive(Debug)]
pub enum NonBlockingSocket {
    Udp(udp::NonBlockingUdpSocket),
    Icmp(icmp::NonBlockingIcmpSocket),
    Pushack(pushack::NonBlockingPushackSocket),
}

impl NonBlockingSocket {
    /// creates a socket based on `protocol` and binds it to `addr` address
    pub fn bind(
        protocol: Protocol,
        addr: &SocketAddr,
        icmp_echo_type: IcmpEchoType,
    ) -> io::Result<Self> {
        let socket = match protocol {
            Protocol::Udp => Self::Udp(udp::NonBlockingUdpSocket::bind(addr)?),
            Protocol::Icmp => Self::Icmp(icmp::NonBlockingIcmpSocket::bind(addr, icmp_echo_type)?),
            Protocol::Pushack => Self::Pushack(pushack::NonBlockingPushackSocket::bind(addr)?),
        };
        Ok(socket)
    }

    pub fn as_mut_udp(&mut self) -> Option<&mut udp::NonBlockingUdpSocket> {
        match self {
            Self::Udp(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn as_mut_pushack(&mut self) -> Option<&mut pushack::NonBlockingPushackSocket> {
        match self {
            Self::Pushack(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn as_udp(&self) -> Option<&udp::NonBlockingUdpSocket> {
        match self {
            Self::Udp(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn as_pushack(&self) -> Option<&pushack::NonBlockingPushackSocket> {
        match self {
            Self::Pushack(inner) => Some(inner),
            _ => None,
        }
    }
}

pub trait NonBlockingSocketTrait {
    fn connect(&mut self, addr: &SocketAddr) -> io::Result<()>;
    fn send(&self, buffer: &mut [u8]) -> io::Result<usize>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
}
impl_enum_deref! { NonBlockingSocket, dyn NonBlockingSocketTrait }

pub mod icmp;
pub(crate) mod pushack;
pub(crate) mod udp;
