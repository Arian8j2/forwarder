mod async_raw;
mod receiver;
pub mod setting;

use super::Socket;
use async_raw::AsyncRawSocket;
use async_trait::async_trait;
use core::panic;
use etherparse::{IcmpEchoHeader, Icmpv4Type};
use lazy_static::lazy_static;
use receiver::{OwnnedData, PacketReceiver, PortListener};
use setting::{IcmpSetting, ICMP_SETTING};
use socket2::{Domain, Protocol, SockAddr};
use std::{
    io::{Error, ErrorKind, Result},
    net::{SocketAddr, SocketAddrV4},
    sync::Mutex,
};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{self, Receiver, Sender};

const ICMPV4_HEADER_LEN_WITHOUT_DATA: usize = 8;
const PACKET_RECEIVER_CHANNEL_QUEUE_SIZE: usize = 128;

lazy_static! {
    pub static ref REGISTER_SENDER: Mutex<Option<Sender<PortListener>>> = Mutex::new(None);
}

/// Udp like socket for Icmp
pub struct IcmpSocket {
    addr: SocketAddrV4,
    socket: AsyncRawSocket,
    receiver: Receiver<OwnnedData>,
    connected_addr: Option<SocketAddr>,
    setting: IcmpSetting,
    // we hold a udp socket with same address as icmp socket to
    // stop using same ports when multiple instance of forwarder is running
    _udp_socket: UdpSocket,
}

impl IcmpSocket {
    pub async fn bind(address: &SocketAddrV4) -> Result<Self> {
        let udp_socket = UdpSocket::bind(address).await?;
        let address = into_socket_addr_v4(udp_socket.local_addr()?)?;

        let socket = AsyncRawSocket::new(Domain::IPV4, Protocol::ICMPV4)?;
        socket.bind(&address.into())?;

        let icmp_setting = ICMP_SETTING
            .lock()
            .map_err(|_| Error::from(ErrorKind::Other))?
            .unwrap();

        let (tx, rx) = mpsc::channel(PACKET_RECEIVER_CHANNEL_QUEUE_SIZE);
        let register_sender = IcmpSocket::get_global_register_receiver(&icmp_setting)?;
        register_sender
            .send(PortListener {
                port: address.port(),
                sender: tx,
            })
            .await
            .unwrap();

        Ok(IcmpSocket {
            socket,
            receiver: rx,
            connected_addr: None,
            addr: address,
            setting: icmp_setting,
            _udp_socket: udp_socket,
        })
    }

    fn get_global_register_receiver(setting: &IcmpSetting) -> Result<Sender<PortListener>> {
        let mut register_sender = REGISTER_SENDER.lock().unwrap();
        if register_sender.is_none() {
            let (packet_receiver, real_register_sender) = PacketReceiver::new(*setting)?;
            packet_receiver.run()?;
            *register_sender = Some(real_register_sender);
        }
        Ok(register_sender.as_ref().unwrap().clone())
    }

    fn craft_icmpv4_packet(
        &self,
        payload: &[u8],
        source_addr: &SocketAddrV4,
        dst_addr: &SocketAddrV4,
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
            self.calc_checksum(bytes5to8, payload).to_be_bytes()
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

    fn calc_checksum(&self, bytes5to8: [u8; 4], payload: &[u8]) -> u16 {
        Icmpv4Type::Unknown {
            code_u8: self.setting.code,
            type_u8: self.setting.icmp_type,
            bytes5to8,
        }
        .calc_checksum(payload)
    }
}

#[async_trait]
impl Socket for IcmpSocket {
    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let Some(from_addr) = self.connected_addr else {
            return Err(ErrorKind::NotConnected.into());
        };

        let SocketAddr::V4(from_v4_addr) = from_addr else {
            unreachable!()
        };

        let data = loop {
            let Some(data) = self.receiver.recv().await else {
                panic!("icmp client channel closed")
            };

            if data.from_addr == from_v4_addr {
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
        Ok((len, data.from_addr.into()))
    }

    async fn send_to(&self, buffer: &[u8], to: &SocketAddr) -> Result<usize> {
        let SocketAddr::V4(v4_addr) = to else {
            unreachable!()
        };
        let packet = self.craft_icmpv4_packet(buffer, &self.addr, v4_addr)?;
        let to_addr = SockAddr::from(to.to_owned());
        self.socket.send_to(packet.as_slice(), &to_addr).await
    }

    async fn send(&self, buffer: &[u8]) -> Result<usize> {
        let Some(to_addr) = self.connected_addr else {
            return Err(ErrorKind::NotConnected.into());
        };
        self.send_to(buffer, &to_addr).await
    }

    async fn connect(&mut self, addr: &SocketAddr) -> Result<()> {
        assert!(addr.is_ipv4());
        self.connected_addr = Some(addr.to_owned());
        Ok(())
    }
}

fn into_socket_addr_v4(socket_addr: SocketAddr) -> Result<SocketAddrV4> {
    match socket_addr {
        SocketAddr::V4(addr) => Ok(addr),
        _ => Err(ErrorKind::Other.into()),
    }
}
