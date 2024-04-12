mod async_raw;
mod ether_helper;
mod receiver;

use super::Socket;
use async_raw::AsyncRawSocket;
use async_trait::async_trait;
use core::panic;
use etherparse::{IcmpEchoHeader, Icmpv4Header, Icmpv4Type, Icmpv6Header, Icmpv6Type};
use lazy_static::lazy_static;
use receiver::{OwnnedData, PacketReceiver, PortListener};
use socket2::{Domain, Protocol, SockAddr};
use std::{
    io::{ErrorKind, Result},
    net::{SocketAddr, SocketAddrV6},
    sync::Mutex,
};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{self, Receiver, Sender};

const PACKET_RECEIVER_CHANNEL_QUEUE_SIZE: usize = 128;

lazy_static! {
    pub static ref REGISTER_SENDER: Mutex<Option<Sender<RegisterMsg>>> = Mutex::new(None);
}

pub enum RegisterMsg {
    Register(PortListener),
    UnRegister { port: u16 },
}

/// Udp like socket for Icmp
pub struct IcmpSocket {
    addr: SocketAddr,
    socket: AsyncRawSocket,
    receiver: Receiver<OwnnedData>,
    connected_addr: Option<SocketAddr>,
    // we hold a udp socket with same address as icmp socket to
    // stop using same ports when multiple instance of forwarder is running
    _udp_socket: UdpSocket,
    register_sender: Sender<RegisterMsg>,
}

impl IcmpSocket {
    pub async fn bind(address: &SocketAddr) -> Result<Self> {
        let udp_socket = UdpSocket::bind(address).await?;
        let address = udp_socket.local_addr()?;

        let socket = IcmpSocket::bind_socket(address)?;
        let (tx, rx) = mpsc::channel(PACKET_RECEIVER_CHANNEL_QUEUE_SIZE);
        let register_sender = IcmpSocket::get_global_register_sender(&address)?;
        let message = RegisterMsg::Register(PortListener {
            port: address.port(),
            sender: tx,
        });
        register_sender.send(message).await.unwrap();

        Ok(IcmpSocket {
            socket,
            receiver: rx,
            connected_addr: None,
            addr: address,
            _udp_socket: udp_socket,
            register_sender,
        })
    }

    pub fn bind_socket(address: SocketAddr) -> Result<AsyncRawSocket> {
        let socket = if address.is_ipv4() {
            AsyncRawSocket::new(Domain::IPV4, Protocol::ICMPV4)
        } else {
            AsyncRawSocket::new(Domain::IPV6, Protocol::ICMPV6)
        }?;
        socket.bind(&address.into())?;
        Ok(socket)
    }

    fn get_global_register_sender(address: &SocketAddr) -> Result<Sender<RegisterMsg>> {
        let mut register_sender = REGISTER_SENDER.lock().unwrap();
        if register_sender.is_none() {
            // receiver has same address of first IcmpSocket for no specific reason
            // just need to make sure that if first IcmpSocket is ipv4 then receiver has to be ipv4
            let (packet_receiver, real_register_sender) = PacketReceiver::new(*address)?;
            packet_receiver.run()?;
            *register_sender = Some(real_register_sender);
        }
        Ok(register_sender.as_ref().unwrap().clone())
    }

    fn craft_icmp_packet(
        &self,
        payload: &[u8],
        source_addr: &SocketAddr,
        dst_addr: &SocketAddr,
    ) -> Result<Vec<u8>> {
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
        Ok(header_and_payload)
    }
}

fn as_socket_addr_v6(socket_addr: SocketAddr) -> SocketAddrV6 {
    match socket_addr {
        SocketAddr::V6(v6_addr) => v6_addr,
        SocketAddr::V4(_) => panic!("as_socket_addr_v6 called on ipv4 address"),
    }
}

#[async_trait]
impl Socket for IcmpSocket {
    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let Some(from_addr) = self.connected_addr else {
            return Err(ErrorKind::NotConnected.into());
        };

        let data = loop {
            let Some(data) = self.receiver.recv().await else {
                panic!("icmp client channel closed")
            };

            if data.from_addr == from_addr {
                break data;
            }
        };

        let len = data.packet.len();
        buffer[..len].copy_from_slice(&data.packet);
        Ok(len)
    }

    async fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddr)> {
        let Some(data) = self.receiver.recv().await else {
            panic!("icmp client channel closed")
        };

        let len = data.packet.len();
        buffer[..len].copy_from_slice(&data.packet);
        Ok((len, data.from_addr))
    }

    async fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> Result<usize> {
        let packet = self.craft_icmp_packet(buffer, &self.addr, to)?;

        // on icmpv6 protocol it returns error when
        // sending icmp packet to destination addr that has port
        let mut to = to.to_owned();
        to.set_port(0);
        let to_addr = SockAddr::from(to);

        self.socket.send_to(packet.as_slice(), &to_addr).await
    }

    async fn send(&self, buffer: &[u8]) -> Result<usize> {
        let Some(to_addr) = self.connected_addr else {
            return Err(ErrorKind::NotConnected.into());
        };
        self.send_to(buffer, &to_addr).await
    }

    async fn connect(&mut self, addr: &SocketAddr) -> Result<()> {
        self.connected_addr = Some(addr.to_owned());
        Ok(())
    }

    fn local_addr(&mut self) -> Result<SocketAddr> {
        Ok(self.addr)
    }
}

impl Drop for IcmpSocket {
    fn drop(&mut self) {
        let sender = self.register_sender.clone();
        let port = self.addr.port();

        tokio::spawn(async move {
            let message = RegisterMsg::UnRegister { port };
            sender.send(message).await.unwrap();
        });
    }
}
