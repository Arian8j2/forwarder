use super::{NonBlockingSocketTrait, SocketTrait};
use smoltcp::wire::{Icmpv4Message, Icmpv4Packet, Icmpv6Message, Icmpv6Packet, IPV4_HEADER_LEN};
use socket2::{Domain, Protocol, Type};
use std::{
    io,
    mem::MaybeUninit,
    net::{IpAddr, Ipv6Addr, SocketAddr},
    slice,
};

pub const ICMP_HEADER_LEN: usize = 8;

pub const ICMP_RESERVED_BYTES_LEN: usize = ICMP_HEADER_LEN + IPV4_HEADER_LEN;

/// `IcmpSocket` that is very similiar to `UdpSocket`
#[derive(Debug)]
pub struct IcmpSocket {
    /// actual underlying icmp socket
    socket: socket2::Socket,
    /// udp socket that is kept alive for avoiding duplicate port
    _udp_socket: std::net::UdpSocket,
    /// address of udp socket same as `udp_socket.local_addr()`
    udp_socket_addr: SocketAddr,
}

impl IcmpSocket {
    pub fn bind(addr: &SocketAddr) -> io::Result<Self> {
        let udp_socket = std::net::UdpSocket::bind(addr)?;
        let udp_socket_addr = udp_socket.local_addr()?;
        let socket = IcmpSocket::inner_bind(*addr)?;

        Ok(IcmpSocket {
            _udp_socket: udp_socket,
            udp_socket_addr,
            socket,
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
}

impl SocketTrait for IcmpSocket {
    fn send_to(&self, buffer: &mut [u8], to: &SocketAddr) -> io::Result<usize> {
        let buffer_with_header = unsafe { slice_sub(buffer, ICMP_HEADER_LEN) };
        craft_icmp_packet(buffer_with_header, &self.udp_socket_addr, to);
        let mut to_addr = *to;
        // in linux `send_to` on icmpv6 socket requires destination port to be zero
        to_addr.set_port(0);
        self.socket.send_to(buffer_with_header, &to_addr.into())
    }

    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let local_addr = self.udp_socket_addr;
        loop {
            let icmp_header_offset = header_offset(local_addr.is_ipv6());

            // aligning in a way that the icmp payload gets written into buffer
            let payload_offset = icmp_header_offset + ICMP_HEADER_LEN;
            let buffer_with_header = unsafe { slice_sub(buffer, payload_offset) };

            let (size, from_addr) = self
                .socket
                .recv_from(cast_maybe_uninit(buffer_with_header))?;
            let icmp_packet = &mut buffer_with_header[icmp_header_offset..size];
            let Some(packet) = parse_icmp_packet(icmp_packet, local_addr.is_ipv6()) else {
                continue;
            };
            if packet.dst_port != local_addr.port() {
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
        Ok(self.udp_socket_addr)
    }
}

#[derive(Debug)]
pub struct NonBlockingIcmpSocket {
    icmp_socket: IcmpSocket,
    // we need to have a copy of connected addr because we
    // need it to craft packet, in ipv6 we need addr + port and
    // in ipv4 we need port
    connected_addr: Option<SocketAddr>,
}

impl NonBlockingIcmpSocket {
    pub fn bind(addr: &SocketAddr) -> io::Result<Self> {
        let icmp_socket = IcmpSocket::bind(addr)?;
        icmp_socket.socket.set_nonblocking(true)?;
        Ok(Self {
            icmp_socket,
            connected_addr: None,
        })
    }
}

impl NonBlockingSocketTrait for NonBlockingIcmpSocket {
    fn recv(&self, _buffer: &mut [u8]) -> io::Result<usize> {
        unreachable!("IcmpPoll doesn't call recv on socket, it has it's own master socket");
    }

    fn send(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let dst_addr = self
            .connected_addr
            .ok_or_else(|| Into::<io::Error>::into(io::ErrorKind::NotConnected))?;
        // it's safe because the main buffer has reserved bytes
        let buffer_with_header = unsafe { slice_sub(buffer, ICMP_HEADER_LEN) };
        craft_icmp_packet(
            buffer_with_header,
            &self.icmp_socket.udp_socket_addr,
            &dst_addr,
        );
        self.icmp_socket.socket.send(buffer_with_header)
    }

    fn connect(&mut self, addr: &SocketAddr) -> io::Result<()> {
        self.connected_addr = Some(*addr);
        let mut addr = *addr;
        // in linux icmpv6 socket requires destination port to be zero
        addr.set_port(0);
        self.icmp_socket.socket.connect(&addr.into())?;
        Ok(())
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.icmp_socket.local_addr()
    }
}

fn craft_icmp_packet(
    buffer_with_header: &mut [u8],
    source_addr: &SocketAddr,
    dst_addr: &SocketAddr,
) {
    let mut icmp_packet = Icmpv4Packet::new_unchecked(buffer_with_header);
    icmp_packet.set_echo_ident(dst_addr.port());
    icmp_packet.set_echo_seq_no(source_addr.port());
    icmp_packet.set_msg_code(0);

    if source_addr.is_ipv4() {
        icmp_packet.set_msg_type(Icmpv4Message::EchoRequest);
        icmp_packet.fill_checksum();
    } else {
        let mut icmp_packet = Icmpv6Packet::new_unchecked(icmp_packet.into_inner());
        icmp_packet.set_msg_type(Icmpv6Message::EchoRequest);
        icmp_packet.fill_checksum(as_ipv6(&source_addr.ip()), as_ipv6(&dst_addr.ip()));
    }
}

pub struct IcmpPacket {
    pub src_port: u16,
    pub dst_port: u16,
}

pub fn parse_icmp_packet(packet: &[u8], is_ipv6: bool) -> Option<IcmpPacket> {
    let icmp_packet = Icmpv4Packet::new_checked(packet).ok()?;

    // we only work with icmp echo requests so if any other type of icmp
    // packet we receive we just ignore it
    let correct_type = if is_ipv6 {
        // icmpv6 echo request
        Icmpv4Message::Unknown(0x80)
    } else {
        Icmpv4Message::EchoRequest
    };
    if icmp_packet.msg_type() != correct_type || icmp_packet.msg_code() != 0 {
        return None;
    }

    // icmp is on layer 3 so it has no idea about ports
    // we use identifier and sequence number of icmp packet as ports
    let dst_port = icmp_packet.echo_ident();
    let src_port = icmp_packet.echo_seq_no();
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

unsafe fn slice_sub(buffer: &mut [u8], count: usize) -> &mut [u8] {
    slice::from_raw_parts_mut(buffer.as_mut_ptr().sub(count), count + buffer.len())
}

pub fn cast_maybe_uninit(buffer: &mut [u8]) -> &mut [MaybeUninit<u8>] {
    // fucking rust with its bullshits
    unsafe { &mut *(buffer as *mut [u8] as *mut [MaybeUninit<u8>]) }
}

fn as_ipv6(ip: &IpAddr) -> &Ipv6Addr {
    match ip {
        IpAddr::V6(ip) => ip,
        _ => panic!(),
    }
}
