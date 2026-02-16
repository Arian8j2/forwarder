use super::{NonBlockingSocketTrait, SocketTrait};
use crate::utils::{cast_maybe_uninit, slice_sub};
use smoltcp::wire::{Ipv4Packet, TcpPacket, TcpSeqNumber, IPV4_HEADER_LEN, TCP_HEADER_LEN};
use socket2::{Domain, Protocol, Type};
use std::{
    io,
    net::{SocketAddr, UdpSocket},
};

/// `PushackSocket` uses raw sockets to send tcp packet with PSH and ACK
/// flags without any proper tcp session handshake beforehand
/// this will mess with some firewalls that only check in handshake process
#[derive(Debug)]
pub struct PushackSocket {
    /// actual underlying icmp socket
    socket: socket2::Socket,
    /// udp socket that is kept alive for avoiding duplicate port
    _udp_socket: UdpSocket,
    addr: SocketAddr,
}

impl PushackSocket {
    pub fn bind(addr: &SocketAddr) -> io::Result<Self> {
        let _udp_socket = UdpSocket::bind(addr)?;
        let addr = _udp_socket.local_addr()?;
        let socket = Self::inner_bind(addr)?;

        #[cfg(target_os = "linux")]
        if let Err(error) =
            socket.attach_filter(&create_bfp_filter(TcpBpfFilter::DstPort(addr.port())))
        {
            log::warn!("couldn't attach bpf filter: {error:?}");
        }

        Ok(Self {
            _udp_socket,
            addr,
            socket,
        })
    }

    pub fn inner_bind(addr: SocketAddr) -> io::Result<socket2::Socket> {
        let socket = socket2::Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::TCP))?;
        socket.bind(&addr.into())?;
        Ok(socket)
    }

    pub fn inner_socket(&self) -> &socket2::Socket {
        &self.socket
    }
}

impl SocketTrait for PushackSocket {
    fn send_to(&self, buffer: &mut [u8], to: &SocketAddr) -> io::Result<usize> {
        let buffer_with_header = unsafe { slice_sub(buffer, TCP_HEADER_LEN) };
        crate_pushack_packet(buffer_with_header, &self.addr, to);
        self.socket.send_to(buffer_with_header, &(*to).into())
    }

    fn recv_from(&self, buffer: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let payload_offset = IPV4_HEADER_LEN + TCP_HEADER_LEN;
        loop {
            let buffer_with_header = unsafe { slice_sub(buffer, payload_offset) };
            let (size, from_addr) = self
                .socket
                .recv_from(cast_maybe_uninit(buffer_with_header))?;
            let Some(packet) = parse_pushack_packet(&buffer_with_header[..size]) else {
                continue;
            };
            if packet.dst_port != self.addr.port() {
                continue;
            }
            // doesn't panic because from_addr is either ipv6 or ipv4
            let mut from_addr = from_addr.as_socket().unwrap();
            from_addr.set_port(packet.src_port);
            let payload_len = size - payload_offset;
            return Ok((payload_len, from_addr));
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.addr)
    }
}

#[derive(Debug)]
pub struct NonBlockingPushackSocket {
    socket: socket2::Socket,
    /// udp socket that is kept alive for avoiding duplicate port
    _udp_socket: UdpSocket,
    addr: SocketAddr,
    // we need to have a copy of connected addr because we
    // need it to craft packet, in ipv6 we need addr + port and
    // in ipv4 we need port
    connected_addr: Option<SocketAddr>,
}

impl NonBlockingPushackSocket {
    pub fn bind(addr: &SocketAddr) -> io::Result<Self> {
        let udp_socket = UdpSocket::bind(addr)?;
        let addr = udp_socket.local_addr()?;
        let socket = PushackSocket::inner_bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            connected_addr: None,
            _udp_socket: udp_socket,
            addr,
        })
    }

    pub fn inner_socket(&self) -> &socket2::Socket {
        &self.socket
    }
}

impl NonBlockingSocketTrait for NonBlockingPushackSocket {
    fn send(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let dst_addr = self
            .connected_addr
            .ok_or_else(|| Into::<io::Error>::into(io::ErrorKind::NotConnected))?;
        // it's safe because the main buffer has reserved bytes
        let buffer_with_header = unsafe { slice_sub(buffer, TCP_HEADER_LEN) };
        crate_pushack_packet(buffer_with_header, &self.addr, &dst_addr);
        self.socket.send(buffer_with_header)
    }

    fn connect(&mut self, addr: &SocketAddr) -> io::Result<()> {
        self.connected_addr = Some(*addr);
        self.socket.connect(&(*addr).into())?;
        Ok(())
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.addr)
    }
}

fn crate_pushack_packet(buffer: &mut [u8], src_addr: &SocketAddr, dst_addr: &SocketAddr) {
    let mut tcp_packet = TcpPacket::new_unchecked(buffer);
    tcp_packet.set_src_port(src_addr.port());
    tcp_packet.set_dst_port(dst_addr.port());
    tcp_packet.set_seq_number(TcpSeqNumber(0));
    tcp_packet.set_ack_number(TcpSeqNumber(0));
    tcp_packet.set_psh(true);
    tcp_packet.set_ack(true);
    tcp_packet.set_window_len(u16::MAX);
    tcp_packet.set_urgent_at(0);
    tcp_packet.set_header_len(TCP_HEADER_LEN as u8);
    tcp_packet.fill_checksum(&src_addr.ip().into(), &dst_addr.ip().into());
}

#[derive(Debug)]
pub struct TcpPacketPort {
    pub src_port: u16,
    pub dst_port: u16,
}

pub fn parse_pushack_packet(packet: &[u8]) -> Option<TcpPacketPort> {
    let ip_header = Ipv4Packet::new_checked(packet).ok()?;
    let ip_payload = ip_header.payload();
    if ip_payload.len() < TCP_HEADER_LEN {
        return None;
    }
    let tcp_packet = TcpPacket::new_unchecked(ip_payload);
    let ports = TcpPacketPort {
        src_port: tcp_packet.src_port(),
        dst_port: tcp_packet.dst_port(),
    };
    Some(ports)
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub enum TcpBpfFilter {
    SrcPort(u16),
    DstPort(u16),
}

#[cfg(target_os = "linux")]
pub fn create_bfp_filter(filter: TcpBpfFilter) -> [libc::sock_filter; 4] {
    let (filter_offset, value) = match filter {
        TcpBpfFilter::SrcPort(port) => (0, port),
        TcpBpfFilter::DstPort(port) => (2, port),
    };
    let total_offset = IPV4_HEADER_LEN + filter_offset;
    [
        (0x28, 0, 0, total_offset as u32), // ldh [offset]
        (0x15, 0, 1, value as u32),        // jne val, drop
        (0x06, 0, 0, 0xffffffff),          // ret #-1
        (0x06, 0, 0, 0000000000),          // drop: ret #0
    ]
    .map(|(code, jt, jf, k)| libc::sock_filter { code, jt, jf, k })
}
