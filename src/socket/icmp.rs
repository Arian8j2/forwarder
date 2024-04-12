mod async_raw;
mod ether_helper;
mod receiver;
pub mod setting;

use super::Socket;
use async_raw::AsyncRawSocket;
use async_trait::async_trait;
use core::panic;
use etherparse::{IcmpEchoHeader, Icmpv4Type, Icmpv6Type};
use lazy_static::lazy_static;
use receiver::{OwnnedData, PacketReceiver, PortListener};
use setting::{IcmpSetting, ICMP_SETTING};
use socket2::{Domain, Protocol, SockAddr};
use std::{
    io::{Error, ErrorKind, Result},
    net::SocketAddr,
    sync::Mutex,
};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{self, Receiver, Sender};

const ICMPV4_HEADER_LEN_WITHOUT_DATA: usize = 8;
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
    setting: IcmpSetting,
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
        let icmp_setting = ICMP_SETTING
            .lock()
            .map_err(|_| Error::from(ErrorKind::Other))?
            .unwrap();

        let (tx, rx) = mpsc::channel(PACKET_RECEIVER_CHANNEL_QUEUE_SIZE);
        let register_sender = IcmpSocket::get_global_register_sender(&icmp_setting, &address)?;
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
            setting: icmp_setting,
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

    fn get_global_register_sender(
        setting: &IcmpSetting,
        address: &SocketAddr,
    ) -> Result<Sender<RegisterMsg>> {
        let mut register_sender = REGISTER_SENDER.lock().unwrap();
        if register_sender.is_none() {
            // receiver has same address of first IcmpSocket for no specific reason
            // just need to make sure that if first IcmpSocket is ipv4 then receiver has to be ipv4
            let (packet_receiver, real_register_sender) = PacketReceiver::new(*setting, *address)?;
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
        let bytes5to8 = IcmpEchoHeader {
            id: dst_addr.port(),
            seq: source_addr.port(),
        }
        .to_bytes();

        let mut result = vec![0u8; ICMPV4_HEADER_LEN_WITHOUT_DATA + payload.len()];
        let checksum = if self.setting.ignore_checksum {
            [0, 0]
        } else {
            match source_addr {
                SocketAddr::V4(_) => self.calc_icmpv4_checksum(bytes5to8, payload).to_be_bytes(),
                SocketAddr::V6(source_addr) => {
                    let source_addr_bytes = source_addr.ip().octets();
                    let SocketAddr::V6(dst_addr) = dst_addr else {
                        unreachable!()
                    };
                    let dst_addr_bytes = dst_addr.ip().octets();
                    self.calc_icmpv6_checksum(bytes5to8, source_addr_bytes, dst_addr_bytes, payload)
                        .to_be_bytes()
                }
            }
        };

        //  0                   1                   2                   3
        //  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |     Type      |     Code      |          Checksum             |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |           Identifier          |        Sequence Number        |
        // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
        // |     Data ...
        // +-+-+-+-+-
        result[0] = self.setting.icmp_type;
        result[1] = self.setting.code;
        result[2..4].copy_from_slice(&checksum);
        result[4..8].copy_from_slice(&bytes5to8);
        result[8..].copy_from_slice(payload);
        Ok(result.to_vec())
    }

    fn calc_icmpv4_checksum(&self, bytes5to8: [u8; 4], payload: &[u8]) -> u16 {
        Icmpv4Type::Unknown {
            code_u8: self.setting.code,
            type_u8: self.setting.icmp_type,
            bytes5to8,
        }
        .calc_checksum(payload)
    }

    fn calc_icmpv6_checksum(
        &self,
        bytes5to8: [u8; 4],
        source_ip: [u8; 16],
        destination_ip: [u8; 16],
        payload: &[u8],
    ) -> u16 {
        Icmpv6Type::Unknown {
            code_u8: self.setting.code,
            type_u8: self.setting.icmp_type,
            bytes5to8,
        }
        .calc_checksum(source_ip, destination_ip, payload)
        .unwrap() // payload is never that large to panic
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
