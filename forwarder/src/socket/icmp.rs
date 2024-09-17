mod ether_helper;
mod receiver;

use crate::MAX_PACKET_SIZE;

use super::SocketTrait;
use etherparse::{IcmpEchoHeader, Icmpv4Header, Icmpv4Type, Icmpv6Header, Icmpv6Type};
use parking_lot::{Mutex, RwLock};
use socket2::{Domain, Protocol, Type};
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    mem::MaybeUninit,
    net::{SocketAddr, SocketAddrV6},
    os::fd::AsRawFd,
    sync::Arc,
};

/// Represents single packet
#[derive(Debug)]
struct Packet {
    data: Vec<u8>,
    from_addr: SocketAddr,
}

/// Thread safe buffer for `Packet`s
type SharedPacketBuffer = Mutex<VecDeque<Packet>>;

/// `Controller` is passed to `IcmpReceiver` so it can communicate to `IcmpSocket`
#[derive(Debug)]
struct Controller {
    packets: Arc<SharedPacketBuffer>,
    waker: mio::Waker,
}

/// `IcmpSocket` that is very similiar to `UdpSocket`
#[derive(Debug)]
pub struct IcmpSocket {
    socket: socket2::Socket,
    /// is underline icmp socket blocking
    is_blocking: bool,
    /// udp socket that is kept alive for avoiding duplicate port
    udp_socket: std::net::UdpSocket,
    /// saves the socket that is connected to
    connected_addr: Option<SocketAddr>,
    /// each `IcmpSocket` does not actually listen for new packets because
    /// icmp protocol is on layer 2 and doesn't have any concept of ports
    /// so each packet will wake up all `IcmpSocket`s, to fix that and remove
    /// overheads of parsing each packet multiple times we listen to packets
    /// only on one socket on another thread and after parsing port and packet
    /// we put it in the corresponding controller `packets`
    packets: Arc<SharedPacketBuffer>,
}

static IS_RECEIVER_STARTED: Mutex<bool> = Mutex::new(false);
static OPEN_PORTS: RwLock<BTreeMap<u16, Controller>> = RwLock::new(BTreeMap::new());

impl IcmpSocket {
    pub fn bind(addr: &SocketAddr) -> io::Result<Self> {
        let udp_socket = std::net::UdpSocket::bind(addr)?;
        let socket = IcmpSocket::inner_bind(addr.clone())?;

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

        let packets = Mutex::new(VecDeque::with_capacity(10));
        Ok(IcmpSocket {
            udp_socket,
            socket,
            packets: Arc::new(packets),
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
        let port = self.udp_socket.local_addr().unwrap().port();
        open_ports.remove(&port);
    }
}

impl SocketTrait for IcmpSocket {
    fn recv(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let (size, _) = self.recv_from(buffer)?;
        Ok(size)
    }

    fn send(&self, buffer: &[u8]) -> io::Result<usize> {
        let dst_addr = self.connected_addr.unwrap();
        let packet = craft_icmp_packet(buffer, &self.local_addr()?, &dst_addr);
        self.socket.send_to(&packet, &dst_addr.into())
    }

    fn connect(&mut self, addr: &SocketAddr) -> io::Result<()> {
        self.socket.connect(&addr.clone().into())?;
        self.connected_addr = Some(*addr);
        Ok(())
    }

    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> io::Result<usize> {
        let packet = craft_icmp_packet(buffer, &self.local_addr()?, to);
        let to_addr = *to;
        self.socket.send_to(&packet, &to_addr.into())
    }

    fn unique_token(&self) -> mio::Token {
        mio::Token(self.udp_socket.as_raw_fd() as usize)
    }

    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        if self.is_blocking {
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
        } else {
            let mut packets = self.packets.lock();
            match packets.pop_front() {
                Some(packet) => {
                    let len = packet.data.len();
                    buffer[..len].copy_from_slice(&packet.data);
                    Ok((len, packet.from_addr))
                }
                None => Err(io::ErrorKind::WouldBlock.into()),
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.udp_socket.local_addr()
    }

    fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        self.socket.set_nonblocking(nonblocking)?;
        self.is_blocking = !nonblocking;
        Ok(())
    }

    fn register(&mut self, registry: &mio::Registry, token: mio::Token) -> io::Result<()> {
        let waker = mio::Waker::new(registry, token)?;
        let mut open_ports = OPEN_PORTS.write();
        let port = self.local_addr()?.port();
        let controller = Controller {
            packets: self.packets.clone(),
            waker,
        };
        open_ports.insert(port, controller);
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
