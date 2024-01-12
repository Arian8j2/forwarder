mod async_raw;
mod receiver;

use super::Socket;
use async_raw::AsyncRawSocket;
use async_trait::async_trait;
use etherparse::{IcmpEchoHeader, Icmpv4Header, Icmpv4Type};
use receiver::{OwnnedData, PacketReceiver, PortIdk, REGISTER_SENDER};
use socket2::{Domain, Protocol, SockAddr};
use std::{
    io::{ErrorKind, Result},
    net::SocketAddrV4,
};
use tokio::sync::mpsc::{self, Receiver};

pub struct IcmpSocket {
    addr: SocketAddrV4,
    socket: AsyncRawSocket,
    receiver: Receiver<OwnnedData>,
    connected_addr: Option<SocketAddrV4>,
}

impl IcmpSocket {
    pub async fn bind(address: &SocketAddrV4) -> Result<Self> {
        let mut address = address.to_owned();
        if address.port() == 0 {
            // TODO: handle duplicate ports
            let random_port: u16 = rand::random();
            address.set_port(random_port);
        }

        let socket = AsyncRawSocket::new(Domain::IPV4, Protocol::ICMPV4)?;
        socket.bind(&address.into())?;

        let mut register_sender = { REGISTER_SENDER.lock().unwrap().to_owned() };
        if register_sender.is_none() {
            let packet_receiver = PacketReceiver::new()?;
            packet_receiver.run()?;
            register_sender = REGISTER_SENDER.lock().unwrap().to_owned();
        }
        let register_sender = register_sender.unwrap();

        let (tx, rx) = mpsc::channel(128);
        register_sender
            .send(PortIdk {
                port: address.port(),
                sender: tx,
            })
            .await
            .unwrap();

        Ok(IcmpSocket {
            socket,
            receiver: rx,
            connected_addr: None,
            addr: address.clone(),
        })
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

    async fn recv_from(&mut self, buffer: &mut [u8]) -> Result<(usize, SocketAddrV4)> {
        let Some(data) = self.receiver.recv().await else {
            panic!("icmp client channel closed")
        };

        let len = data.packet.len();
        buffer[..len].copy_from_slice(&data.packet);
        Ok((len, data.from_addr))
    }

    async fn send_to(&self, buffer: &[u8], to: &SocketAddrV4) -> Result<usize> {
        let packet = craft_icmpv4_packet(buffer, &self.addr, to)?;
        let to_addr = SockAddr::from(to.to_owned());
        self.socket.send_to(packet.as_slice(), &to_addr).await
    }

    async fn send(&self, buffer: &[u8]) -> Result<usize> {
        let Some(to_addr) = self.connected_addr else {
            return Err(ErrorKind::NotConnected.into());
        };
        self.send_to(buffer, &to_addr).await
    }

    async fn connect(&mut self, addr: &SocketAddrV4) -> Result<()> {
        self.connected_addr = Some(addr.to_owned());
        Ok(())
    }
}

fn craft_icmpv4_packet(
    payload: &[u8],
    source_addr: &SocketAddrV4,
    dst_addr: &SocketAddrV4,
) -> Result<Vec<u8>> {
    let echo_header = IcmpEchoHeader {
        id: dst_addr.port(),
        seq: source_addr.port(),
    };
    let packet =
        Icmpv4Header::with_checksum(Icmpv4Type::EchoRequest(echo_header), payload).to_bytes();
    assert_eq!(packet.len(), 8);

    let mut result = Vec::with_capacity(packet.len() + payload.len());
    unsafe { result.set_len(result.capacity()) };
    result[..packet.len()].copy_from_slice(&packet);
    result[packet.len()..].copy_from_slice(&payload);
    Ok(result.to_vec())
}
