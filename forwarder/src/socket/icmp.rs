use super::{NonBlockingSocketTrait, SocketTrait};
use crate::utils::{cast_maybe_uninit, slice_sub};
use smoltcp::wire::{Icmpv4Message, Icmpv4Packet, Icmpv6Message, Icmpv6Packet, IPV4_HEADER_LEN};
use socket2::{Domain, Protocol, Type};
use std::{
    io,
    net::{IpAddr, Ipv6Addr, SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

pub const ICMP_HEADER_LEN: usize = 8;

pub const ICMP_RESERVED_BYTES_LEN: usize = ICMP_HEADER_LEN + IPV4_HEADER_LEN;

/// `IcmpSocket` that is very similiar to `UdpSocket`
#[derive(Debug)]
pub struct IcmpSocket {
    /// actual underlying icmp socket
    socket: socket2::Socket,
    /// udp socket that is kept alive for avoiding duplicate port
    _udp_socket: Option<UdpSocket>,
    addr: SocketAddr,
    /// the type of icmp echo packets that this socket receives
    icmp_echo_type: IcmpEchoType,
    /// read timeout
    read_timeout: Option<Duration>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum IcmpEchoType {
    Request,
    Reply,
}

impl IcmpEchoType {
    pub fn opposite(&self) -> Self {
        match self {
            Self::Request => Self::Reply,
            Self::Reply => Self::Request,
        }
    }
}

impl IcmpSocket {
    pub fn bind(
        addr: &SocketAddr,
        icmp_echo_type: IcmpEchoType,
        check_port: bool,
    ) -> io::Result<Self> {
        let (udp_socket, udp_socket_addr) = if check_port {
            let udp_socket = UdpSocket::bind(addr)?;
            let addr = udp_socket.local_addr()?;
            (Some(udp_socket), addr)
        } else {
            (None, *addr)
        };
        let socket = IcmpSocket::inner_bind(*addr)?;

        // TODO: maybe attach a bpf filter here

        Ok(IcmpSocket {
            _udp_socket: udp_socket,
            addr: udp_socket_addr,
            socket,
            icmp_echo_type,
            read_timeout: None,
        })
    }

    pub fn inner_bind(addr: SocketAddr) -> io::Result<socket2::Socket> {
        let socket = if addr.is_ipv4() {
            socket2::Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))
        } else {
            socket2::Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))
        }?;
        socket.bind(&addr.into())?;
        Ok(socket)
    }

    pub fn inner_socket(&self) -> &socket2::Socket {
        &self.socket
    }

    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.read_timeout = timeout;
        self.socket.set_read_timeout(timeout)
    }
}

impl SocketTrait for IcmpSocket {
    fn send_to(&self, buffer: &mut [u8], to: &SocketAddr) -> io::Result<usize> {
        let buffer_with_header = unsafe { slice_sub(buffer, ICMP_HEADER_LEN) };
        craft_icmp_packet(
            buffer_with_header,
            &self.addr,
            to,
            self.icmp_echo_type.opposite(),
        );
        let mut to_addr = *to;
        // in linux `send_to` on icmpv6 socket requires destination port to be zero
        to_addr.set_port(0);
        self.socket.send_to(buffer_with_header, &to_addr.into())
    }

    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let is_ipv6 = self.addr.is_ipv6();
        let icmp_header_offset = header_offset(is_ipv6);
        // aligning in a way that the icmp payload gets written into buffer
        let payload_offset = icmp_header_offset + ICMP_HEADER_LEN;

        let started = self.read_timeout.map(|timeout| (Instant::now(), timeout));
        loop {
            if started.is_some_and(|(started, timeout)| started.elapsed() > timeout) {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let buffer_with_header = unsafe { slice_sub(buffer, payload_offset) };
            let (size, from_addr) = self
                .socket
                .recv_from(cast_maybe_uninit(buffer_with_header))?;
            let icmp_packet = &mut buffer_with_header[icmp_header_offset..size];
            let Some(packet) = parse_icmp_packet(icmp_packet, is_ipv6, self.icmp_echo_type) else {
                continue;
            };
            if packet.dst_port != self.addr.port() {
                continue;
            }
            // doesn't panic because from_addr is either ipv6 or ipv4
            let mut from_addr = from_addr.as_socket().unwrap();
            from_addr.set_port(packet.src_port);
            let payload_len = size - icmp_header_offset - ICMP_HEADER_LEN;
            return Ok((payload_len, from_addr));
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.addr)
    }
}

#[derive(Debug)]
pub struct NonBlockingIcmpSocket {
    socket: socket2::Socket,
    /// udp socket that is kept alive for avoiding duplicate port
    _udp_socket: UdpSocket,
    addr: SocketAddr,
    // we need to have a copy of connected addr because we
    // need it to craft packet, in ipv6 we need addr + port and
    // in ipv4 we need port
    connected_addr: Option<SocketAddr>,
    /// the type of icmp echo packets that this socket receives
    echo_type: IcmpEchoType,
}

impl NonBlockingIcmpSocket {
    pub fn bind(addr: &SocketAddr, echo_type: IcmpEchoType) -> io::Result<Self> {
        let udp_socket = UdpSocket::bind(addr)?;
        let addr = udp_socket.local_addr()?;
        let socket = IcmpSocket::inner_bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            connected_addr: None,
            _udp_socket: udp_socket,
            addr,
            echo_type,
        })
    }
}

impl NonBlockingSocketTrait for NonBlockingIcmpSocket {
    fn send(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let dst_addr = self
            .connected_addr
            .ok_or_else(|| Into::<io::Error>::into(io::ErrorKind::NotConnected))?;
        // it's safe because the main buffer has reserved bytes
        let buffer_with_header = unsafe { slice_sub(buffer, ICMP_HEADER_LEN) };
        craft_icmp_packet(
            buffer_with_header,
            &self.addr,
            &dst_addr,
            self.echo_type.opposite(),
        );
        self.socket.send(buffer_with_header)
    }

    fn connect(&mut self, addr: &SocketAddr) -> io::Result<()> {
        self.connected_addr = Some(*addr);
        let mut addr = *addr;
        // in linux icmpv6 socket requires destination port to be zero
        addr.set_port(0);
        self.socket.connect(&addr.into())?;
        Ok(())
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.addr)
    }
}

fn craft_icmp_packet(
    buffer_with_header: &mut [u8],
    src_addr: &SocketAddr,
    dst_addr: &SocketAddr,
    echo_type: IcmpEchoType,
) {
    let mut icmp_packet = Icmpv4Packet::new_unchecked(buffer_with_header);
    // point of this is to make sure echo ident of request and corresponding reply
    // remains the same, so nat could figure out who are we talking to
    let is_echo_reply = echo_type == IcmpEchoType::Reply;
    let (ident, seq) = if is_echo_reply {
        (dst_addr.port(), src_addr.port())
    } else {
        (src_addr.port(), dst_addr.port())
    };
    icmp_packet.set_echo_ident(ident);
    icmp_packet.set_echo_seq_no(seq);
    icmp_packet.set_msg_code(0);

    if src_addr.is_ipv4() {
        icmp_packet.set_msg_type(if is_echo_reply {
            Icmpv4Message::EchoReply
        } else {
            Icmpv4Message::EchoRequest
        });
        icmp_packet.fill_checksum();
    } else {
        let mut icmp_packet = Icmpv6Packet::new_unchecked(icmp_packet.into_inner());
        icmp_packet.set_msg_type(if is_echo_reply {
            Icmpv6Message::EchoReply
        } else {
            Icmpv6Message::EchoRequest
        });
        icmp_packet.fill_checksum(as_ipv6(&src_addr.ip()), as_ipv6(&dst_addr.ip()));
    }
}

pub struct IcmpPacket {
    pub src_port: u16,
    pub dst_port: u16,
}

pub fn parse_icmp_packet(
    packet: &[u8],
    is_ipv6: bool,
    echo_type: IcmpEchoType,
) -> Option<IcmpPacket> {
    let icmp_packet = Icmpv4Packet::new_checked(packet).ok()?;
    let is_echo_reply = echo_type == IcmpEchoType::Reply;

    // we only work with icmp echo requests so if any other type of icmp
    // packet we receive we just ignore it
    // TODO: maybe always check for both reply or request
    let correct_type = if is_ipv6 {
        if is_echo_reply {
            // icmpv6 echo reply
            Icmpv4Message::Unknown(0x81)
        } else {
            // icmpv6 echo request
            Icmpv4Message::Unknown(0x80)
        }
    } else if is_echo_reply {
        Icmpv4Message::EchoReply
    } else {
        Icmpv4Message::EchoRequest
    };
    if icmp_packet.msg_type() != correct_type || icmp_packet.msg_code() != 0 {
        return None;
    }

    // icmp is on layer 3 so it has no idea about ports
    // we use identifier and sequence number of icmp packet as ports
    let ident = icmp_packet.echo_ident();
    let seq = icmp_packet.echo_seq_no();

    let (src_port, dst_port) = if is_echo_reply {
        (seq, ident)
    } else {
        (ident, seq)
    };
    Some(IcmpPacket { src_port, dst_port })
}

pub fn header_offset(is_ipv6: bool) -> usize {
    // in icmpv4 when calling recv the kernel will include ipv4 header
    // in the buffer but for icmpv6 this is not the case
    if is_ipv6 {
        0
    } else {
        IPV4_HEADER_LEN
    }
}

fn as_ipv6(ip: &IpAddr) -> &Ipv6Addr {
    match ip {
        IpAddr::V6(ip) => ip,
        _ => panic!(),
    }
}

// bpf filter is really useful when having multiple forwarder instances
// on same kernel, by default all icmp sockets receives all packets and then
// we in user mode filter them but with this the packets get filtered on kernel
#[cfg(target_os = "linux")]
pub fn create_bfp_filter(is_ipv6: bool, ty: IcmpEchoType, value: u16) -> [libc::sock_filter; 4] {
    let icmp_header_offset = header_offset(is_ipv6);
    let field_offset = if ty == IcmpEchoType::Reply { 6 } else { 4 };
    let offset = field_offset + icmp_header_offset;
    [
        (0x28, 0, 0, offset as u32), // ldh [offset]
        (0x15, 0, 1, value as u32),  // jne val, drop
        (0x06, 0, 0, 0xffffffff),    // ret #-1
        (0x06, 0, 0, 0000000000),    // drop: ret #0
    ]
    .map(|(code, jt, jf, k)| libc::sock_filter { code, jt, jf, k })
}
