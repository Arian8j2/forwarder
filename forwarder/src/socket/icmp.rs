mod ether_helper;
mod receiver;

use super::SocketTrait;
use crate::MAX_PACKET_SIZE;
use etherparse::{IcmpEchoHeader, Icmpv4Header, Icmpv4Type, Icmpv6Header, Icmpv6Type};
use mio::{unix::SourceFd, Interest};
use parking_lot::{Mutex, RwLock};
use socket2::{Domain, Protocol, Type};
use std::{
    collections::BTreeSet,
    io,
    mem::MaybeUninit,
    net::{SocketAddr, SocketAddrV6},
    os::fd::AsRawFd,
};

/// `IcmpSocket` that is very similiar to `UdpSocket`
#[derive(Debug)]
pub struct IcmpSocket {
    socket: socket2::Socket,
    /// is underline icmp socket blocking
    is_blocking: bool,
    /// udp socket that is kept alive for avoiding duplicate port
    udp_socket: std::net::UdpSocket,
    /// address of udp socket
    udp_socket_addr: SocketAddr,
    /// saves the socket that is connected to
    connected_addr: Option<SocketAddr>,
}

static IS_RECEIVER_STARTED: Mutex<bool> = Mutex::new(false);

/// each nonblocking `IcmpSocket` does not actually listen for new packets because
/// icmp protocol is on layer 2 and doesn't have any concept of ports
/// so each packet will wake up all `IcmpSocket`s, to fix that and remove
/// overheads of parsing each packet multiple times we listen to packets
/// only on one socket on another thread and after parsing port and packet
/// we put it in the corresponding controller `packets`, each nonblocking
/// `IcmpSocket` can register it's port via adding it to `OPEN_PORTS`
static OPEN_PORTS: RwLock<BTreeSet<u16>> = RwLock::new(BTreeSet::new());

impl IcmpSocket {
    pub fn bind(addr: &SocketAddr) -> io::Result<Self> {
        let udp_socket = std::net::UdpSocket::bind(addr)?;
        let udp_socket_addr = udp_socket.local_addr()?;
        let socket = IcmpSocket::inner_bind(*addr)?;

        // run the icmp receiver if it isn't running
        let mut is_receiver_alive = IS_RECEIVER_STARTED.lock();
        if !*is_receiver_alive {
            let addr_clone = addr.to_owned();
            std::thread::spawn(move || {
                if let Err(error) = receiver::run_icmp_receiver(addr_clone) {
                    log::error!("icmp receiver exited with error: {error:?}")
                }
                panic!("icmp receiver")
            });
            *is_receiver_alive = true;
        }

        Ok(IcmpSocket {
            udp_socket,
            udp_socket_addr,
            socket,
            connected_addr: None,
            is_blocking: true,
        })
    }

    fn inner_bind(addr: SocketAddr) -> io::Result<socket2::Socket> {
        let socket = if addr.is_ipv4() {
            // TODO: why Type::RAW, why not Type::DGRAM
            socket2::Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))
        } else {
            socket2::Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))
        }?;
        socket.bind(&addr.into())?;
        Ok(socket)
    }
}

impl Drop for IcmpSocket {
    fn drop(&mut self) {
        // clear port
        let mut open_ports = OPEN_PORTS.write();
        open_ports.remove(&self.udp_socket_addr.port());
    }
}

impl SocketTrait for IcmpSocket {
    fn recv(&self, buffer: &mut [u8]) -> io::Result<usize> {
        assert!(
            !self.is_blocking,
            "IcmpSocket::recv was called in blocking mode"
        );
        // icmp receiver sends packets that it receives to udp socket of `IcmpSocket`
        let (size, from_addr) = self.udp_socket.recv_from(buffer)?;
        // make sure that the receiver sent the packet
        // receiver is local so the packet ip is from loopback
        if from_addr.ip().is_loopback() {
            Ok(size)
        } else {
            Err(io::ErrorKind::ConnectionRefused.into())
        }
    }

    fn send(&self, buffer: &[u8]) -> io::Result<usize> {
        let dst_addr = self.connected_addr.unwrap();
        let packet = craft_icmp_packet(buffer, &self.local_addr()?, &dst_addr);
        let dst_addr: SocketAddr = if dst_addr.is_ipv6() {
            // in linux `send_to` on icmpv6 socket requires dst address port to be zero
            let mut addr_without_port = dst_addr;
            addr_without_port.set_port(0);
            addr_without_port
        } else {
            dst_addr
        };
        self.socket.send_to(&packet, &dst_addr.into())
    }

    fn connect(&mut self, addr: &SocketAddr) -> io::Result<()> {
        let addr = *addr;
        self.socket.connect(&addr.into())?;
        self.connected_addr = Some(addr);
        Ok(())
    }

    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> io::Result<usize> {
        let packet = craft_icmp_packet(buffer, &self.local_addr()?, to);
        let mut to_addr = *to;
        // in linux `send_to` on icmpv6 socket requires dst address port to be zero
        to_addr.set_port(0);
        self.socket.send_to(&packet, &to_addr.into())
    }

    fn unique_token(&self) -> mio::Token {
        mio::Token(self.udp_socket.as_raw_fd() as usize)
    }

    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        assert!(
            self.is_blocking,
            "IcmpSocket::recv_from was called in non blocking mode"
        );
        let mut second_buffer = [0u8; MAX_PACKET_SIZE];
        let local_addr = self.local_addr()?;
        loop {
            let (size, from_addr) = self.socket.recv_from(unsafe {
                &mut *(&mut second_buffer as *mut [u8] as *mut [MaybeUninit<u8>])
            })?;
            let Some(packet) =
                receiver::parse_icmp_packet(&second_buffer[..size], local_addr.is_ipv6())
            else {
                continue;
            };
            if packet.dst_port != local_addr.port() {
                continue;
            }
            let payload_len = packet.payload.len();
            buffer[..payload_len].copy_from_slice(packet.payload);

            let mut from_addr = from_addr.as_socket().unwrap();
            from_addr.set_port(packet.src_port);
            return Ok((payload_len, from_addr));
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.udp_socket_addr)
    }

    fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        self.socket.set_nonblocking(nonblocking)?;
        self.udp_socket.set_nonblocking(nonblocking)?;
        self.is_blocking = !nonblocking;
        Ok(())
    }

    fn register(&mut self, registry: &mio::Registry, token: mio::Token) -> io::Result<()> {
        let mut open_ports = OPEN_PORTS.write();
        open_ports.insert(self.udp_socket_addr.port());

        registry.register(
            &mut SourceFd(&self.udp_socket.as_raw_fd()),
            token,
            Interest::READABLE,
        )?;
        Ok(())
    }
}

fn craft_icmp_packet(payload: &[u8], source_addr: &SocketAddr, dst_addr: &SocketAddr) -> Vec<u8> {
    let echo_header = IcmpEchoHeader {
        id: dst_addr.port(),
        seq: source_addr.port(),
    };

    // TODO: rewrite this part to use fewer allocations
    let icmp_header = if source_addr.is_ipv4() {
        let icmp_type = Icmpv4Type::EchoRequest(echo_header);
        Icmpv4Header::with_checksum(icmp_type, payload)
            .to_bytes()
            .to_vec()
    } else {
        let icmp_type = Icmpv6Type::EchoRequest(echo_header);
        let source_ip = as_socket_addr_v6(*source_addr).ip().octets();
        let destination_ip = as_socket_addr_v6(*dst_addr).ip().octets();
        Icmpv6Header::with_checksum(icmp_type, source_ip, destination_ip, payload)
            .unwrap()
            .to_bytes()
            .to_vec()
    };

    let mut header_and_payload = Vec::with_capacity(icmp_header.len() + payload.len());
    header_and_payload.extend_from_slice(&icmp_header);
    header_and_payload.extend_from_slice(payload);
    header_and_payload
}

fn as_socket_addr_v6(socket_addr: SocketAddr) -> SocketAddrV6 {
    match socket_addr {
        SocketAddr::V6(v6_addr) => v6_addr,
        SocketAddr::V4(_) => panic!("as_socket_addr_v6 called on ipv4 address"),
    }
}
