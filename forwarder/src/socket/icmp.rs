mod ether_helper;

use super::{NonBlockingSocketTrait, SocketTrait};
use crate::MAX_PACKET_SIZE;
use ether_helper::IcmpSlice;
use etherparse::{
    IcmpEchoHeader, Icmpv4Header, Icmpv4Type, Icmpv6Header, Icmpv6Type, Ipv4HeaderSlice,
};
use socket2::{Domain, Protocol, Type};
use std::{
    io,
    mem::MaybeUninit,
    net::{SocketAddr, SocketAddrV6},
};

/// magic bytes that are injected to end of icmp echo reply packets that
/// we craft and it get discarded later when parsing, it's purpose is to
/// detect automatic echo reply packets of kernel and ignore them
const ECHO_REPLY_MAGIC: [u8; 3] = [0x24, 0x74, 0x33];

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
    fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> io::Result<usize> {
        let packet = craft_icmp_packet(buffer, &self.udp_socket_addr, to, false)?;
        let mut to_addr = *to;
        // in linux `send_to` on icmpv6 socket requires destination port to be zero
        to_addr.set_port(0);
        self.socket.send_to(&packet, &to_addr.into())
    }

    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let mut second_buffer = [0u8; MAX_PACKET_SIZE];
        let local_addr = self.local_addr()?;
        loop {
            let (size, from_addr) = self.socket.recv_from(unsafe {
                &mut *(&mut second_buffer as *mut [u8] as *mut [MaybeUninit<u8>])
            })?;
            let Some(packet) = parse_icmp_packet(&mut second_buffer[..size], local_addr.is_ipv6())
            else {
                continue;
            };
            if packet.dst_port != local_addr.port() {
                continue;
            }
            let payload_len = packet.payload.len();
            buffer[..payload_len].copy_from_slice(packet.payload);

            // doesn't panic because from_addr is either ipv6 or ipv4
            let mut from_addr = from_addr.as_socket().unwrap();
            from_addr.set_port(packet.src_port);
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
    // int ipv4 we need port
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

    fn send(&self, buffer: &[u8]) -> io::Result<usize> {
        let dst_addr = self
            .connected_addr
            .ok_or_else(|| Into::<io::Error>::into(io::ErrorKind::NotConnected))?;
        let packet = craft_icmp_packet(buffer, &self.icmp_socket.udp_socket_addr, &dst_addr, true)?;
        self.icmp_socket.socket.send(&packet)
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
    payload: &[u8],
    source_addr: &SocketAddr,
    dst_addr: &SocketAddr,
    is_echo_request: bool,
) -> io::Result<Vec<u8>> {
    // when we are sending echo reply we inject few magic bytes to the
    // end of payload so when receiving reply packets we can determine
    // if the echo reply packet is automatically sent from kernel
    // (in case /proc/sys/net/ipv4/icmp_echo_ignore_all is not turned off)
    // or we actually sent it
    let payload = if !is_echo_request {
        let payload_with_magic_len = payload.len() + ECHO_REPLY_MAGIC.len();
        // TODO: this allocation is really bad, find another way for it
        let mut buffer = vec![0u8; payload_with_magic_len];
        buffer[..payload.len()].copy_from_slice(payload);
        buffer[payload.len()..].copy_from_slice(&ECHO_REPLY_MAGIC);
        buffer
    } else {
        payload.to_vec()
    };

    // read comments on `receiver::parse_icmp_packet` on why the
    // source and destination place changes based on echo reply or request
    let echo_header = if is_echo_request {
        IcmpEchoHeader {
            id: source_addr.port(),
            seq: dst_addr.port(),
        }
    } else {
        IcmpEchoHeader {
            id: dst_addr.port(),
            seq: source_addr.port(),
        }
    };

    let icmp_header = if source_addr.is_ipv4() {
        let icmp_type = if is_echo_request {
            Icmpv4Type::EchoRequest(echo_header)
        } else {
            Icmpv4Type::EchoReply(echo_header)
        };
        Icmpv4Header::with_checksum(icmp_type, &payload)
            .to_bytes()
            .to_vec()
    } else {
        let icmp_type = if is_echo_request {
            Icmpv6Type::EchoRequest(echo_header)
        } else {
            Icmpv6Type::EchoReply(echo_header)
        };
        let source_ip = as_socket_addr_v6(*source_addr).ip().octets();
        let destination_ip = as_socket_addr_v6(*dst_addr).ip().octets();
        Icmpv6Header::with_checksum(icmp_type, source_ip, destination_ip, &payload)
            .map_err(|_| Into::<io::Error>::into(io::ErrorKind::InvalidInput))?
            .to_bytes()
            .to_vec()
    };

    let mut header_and_payload = Vec::with_capacity(icmp_header.len() + payload.len());
    header_and_payload.extend_from_slice(&icmp_header);
    header_and_payload.extend_from_slice(&payload);
    Ok(header_and_payload)
}

pub struct IcmpPacket<'a> {
    pub payload: &'a mut [u8],
    pub src_port: u16,
    pub dst_port: u16,
}

pub fn parse_icmp_packet(packet: &mut [u8], is_ipv6: bool) -> Option<IcmpPacket<'_>> {
    // according to 'icmp6' man page on freebsd (seems like linux does this too):
    // 'Incoming packets on the socket are received with the IPv6 header and any extension headers removed'
    //
    // but on 'icmp' man page that is for icmpv4, it says:
    // 'Incoming packets are received with the IP header and options intact.'
    //
    // so we need to parse header in icmpv4 but not in icmpv6
    let payload_start_index = if is_ipv6 {
        0
    } else {
        let ip_header = Ipv4HeaderSlice::from_slice(packet).ok()?;
        let payload_len: usize = ip_header.payload_len().into();
        packet.len() - payload_len
    };

    let icmp = IcmpSlice::from_slice(is_ipv6, &packet[payload_start_index..])?;
    // we only work with icmp echo requests and replies so if any other type of icmp
    // packet we receive we just ignore it
    let echo_request = if is_ipv6 {
        etherparse::icmpv6::TYPE_ECHO_REQUEST
    } else {
        etherparse::icmpv4::TYPE_ECHO_REQUEST
    };
    let echo_reply = if is_ipv6 {
        etherparse::icmpv6::TYPE_ECHO_REPLY
    } else {
        etherparse::icmpv4::TYPE_ECHO_REPLY
    };

    let is_echo_request = icmp.type_u8() == echo_request;
    let is_echo_reply = icmp.type_u8() == echo_reply;

    let is_correct_icmp_type = is_echo_request || is_echo_reply;
    if !is_correct_icmp_type || icmp.code_u8() != 0 {
        return None;
    }

    let bytes5to8 = icmp.bytes5to8();
    let id = u16::from_be_bytes([bytes5to8[0], bytes5to8[1]]);
    let seq = u16::from_be_bytes([bytes5to8[2], bytes5to8[3]]);

    let payload_len = icmp.payload().len();
    let packet_len = packet.len();

    let payload = if is_echo_request {
        &mut packet[packet_len - payload_len..]
    } else {
        // filter the reply packets that doesn't have the magic bytes
        let payload = &mut packet[packet_len - payload_len..];
        let magic_len = ECHO_REPLY_MAGIC.len();
        if payload_len < magic_len {
            return None;
        }
        if payload[payload_len - magic_len..] != ECHO_REPLY_MAGIC {
            return None;
        }
        // striping magic bytes off the payload
        &mut payload[..payload_len - magic_len]
    };

    // icmp is on layer 3 so it has no idea about ports so we use
    // identification and sequence part of icmp packet as src and dst port
    // but the problem is that if for example port 1010 sends a packet to 8000
    // the id and seq is like this:
    //      | ID: 1010 | SEQ: 8000 |
    // now if the server wants to send echo reply from 8000 to 1010 the packet
    // will be like this:
    //      | ID: 8000 | SEQ: 1010 |
    // because now the sender is 8000 and the receiver is 1010
    // the important part is the ID, if the ID of echo reply is different than
    // the echo request then NAT has no clue how to forward this packet, so we
    // swap the id and seq position based on the packet being reply or request
    // so the ID field will not get changed
    let (src_port, dst_port) = if is_echo_reply { (seq, id) } else { (id, seq) };

    Some(IcmpPacket {
        payload,
        src_port,
        dst_port,
    })
}

fn as_socket_addr_v6(socket_addr: SocketAddr) -> SocketAddrV6 {
    match socket_addr {
        SocketAddr::V6(v6_addr) => v6_addr,
        SocketAddr::V4(_) => panic!("as_socket_addr_v6 called on ipv4 address"),
    }
}
