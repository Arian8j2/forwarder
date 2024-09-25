use super::{ether_helper::IcmpSlice, IcmpSocket, OPEN_PORTS};
use crate::MAX_PACKET_SIZE;
use etherparse::Ipv4HeaderSlice;
use std::{mem::MaybeUninit, net::SocketAddr};

// each nonblocking `IcmpSocket` does not actually listen for new packets because
// icmp protocol is on layer 3 and doesn't have any concept of ports
// so if each `IcmpSocket` called `recv` each packet that comes through icmp will wake up all `IcmpSocket`s
// to fix that and remove overheads of parsing each packet multiple times we listen to packets
// only on one socket on another thread and after parsing port and packet
// we send it back to `IcmpSocket` via udp protocol
pub fn run_icmp_receiver(addr: SocketAddr) -> anyhow::Result<()> {
    let is_ipv6 = addr.is_ipv6();
    let socket: socket2::Socket = IcmpSocket::inner_bind(addr)?;
    let udp_socket = std::net::UdpSocket::bind(SocketAddr::new(addr.ip(), 0))?;
    udp_socket.set_nonblocking(true)?;

    let mut buffer = [0u8; MAX_PACKET_SIZE];
    let mut addr_buffer = addr;
    let open_ports = &OPEN_PORTS[is_ipv6 as usize];

    loop {
        let Ok(size) =
            socket.recv(unsafe { &mut *(&mut buffer as *mut [u8] as *mut [MaybeUninit<u8>]) })
        else {
            continue;
        };
        let Some(icmp_packet) = parse_icmp_packet(&buffer[..size], is_ipv6) else {
            continue;
        };
        let open_ports = open_ports.read();
        let port = icmp_packet.dst_port;
        if open_ports.contains(&port) {
            addr_buffer.set_port(port);
            udp_socket.send_to(icmp_packet.payload, addr_buffer).ok();
        }
    }
}

pub struct IcmpPacket<'a> {
    pub payload: &'a [u8],
    pub src_port: u16,
    pub dst_port: u16,
}

pub fn parse_icmp_packet(packet: &[u8], is_ipv6: bool) -> Option<IcmpPacket<'_>> {
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
    // we only work with icmp echo requests so if any other type of icmp
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

    if !(icmp.type_u8() == echo_request || icmp.type_u8() == echo_reply) {
        return None;
    }
    if icmp.code_u8() != 0 {
        return None;
    }

    let bytes5to8 = icmp.bytes5to8();
    let id = u16::from_be_bytes([bytes5to8[0], bytes5to8[1]]);
    let seq = u16::from_be_bytes([bytes5to8[2], bytes5to8[3]]);

    let payload_len = icmp.payload().len();
    let payload = if icmp.type_u8() == echo_request {
        &packet[packet.len() - payload_len..]
    } else {
        // filter the reply packets that doesn't have the magic bytes
        let payload = &packet[packet.len() - payload_len..];
        let magic_len = super::ECHO_REPLY_MAGIC.len();
        if payload.len() < magic_len {
            return None;
        }
        if payload[payload.len() - magic_len..] != super::ECHO_REPLY_MAGIC {
            return None;
        }
        // striping magic bytes off the payload
        &payload[..payload.len() - magic_len]
    };

    // icmp is on layer 3 so it has no idea about ports so we use
    // identification and sequence part of icmp packet as src and dst port
    // but the problem is that if for example port 1010 sends a packet to 8000
    // the id and seq is like this:
    //      | ID: 1010 | SEQ: 8000 |
    // now if the server wants to send echo reply from 8000 to 1010 the packet
    // will be like this:
    //      | ID: 8000 | SEQ: 1010 |
    // the important part is the ID, if the ID of echo reply is different than
    // the echo request then NAT has no clue how to forward this packet, so we
    // swap the id and seq position based on the packet is reply or request
    let (src_port, dst_port) = if icmp.type_u8() == echo_reply {
        (seq, id)
    } else {
        (id, seq)
    };

    Some(IcmpPacket {
        payload,
        src_port,
        dst_port,
    })
}
