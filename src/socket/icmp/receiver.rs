use super::AsyncRawSocket;
use crate::macros::loop_select;
use etherparse::{Icmpv4Slice, Ipv4HeaderSlice};
use lazy_static::lazy_static;
use log::info;
use socket2::{Domain, Protocol};
use std::io::ErrorKind;
use std::net::SocketAddrV4;
use std::sync::Mutex;
use std::{io::Result, net::Ipv4Addr};
use tokio::sync::mpsc::{self, Receiver, Sender};

lazy_static! {
    pub static ref REGISTER_SENDER: Mutex<Option<Sender<PortIdk>>> = Mutex::new(None);
}

pub struct PortIdk {
    pub port: u16,
    pub sender: Sender<OwnnedData>,
}

#[derive(Debug)]
pub struct OwnnedData {
    pub from_addr: SocketAddrV4,
    pub packet: Vec<u8>,
}

pub struct PacketReceiver {
    socket: AsyncRawSocket,
    open_ports: Vec<PortIdk>,
    register_receiver: Receiver<PortIdk>,
}

impl PacketReceiver {
    pub fn new() -> Result<Self> {
        let socket = AsyncRawSocket::new(Domain::IPV4, Protocol::ICMPV4)?;
        let adress = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0);
        socket.bind(&adress.into())?;

        let (tx, rx) = mpsc::channel::<PortIdk>(256);
        let mut register_sender = REGISTER_SENDER.lock().unwrap();
        *register_sender = Some(tx);
        info!("new icmp packet receiver");

        Ok(PacketReceiver {
            socket,
            open_ports: Vec::with_capacity(50),
            register_receiver: rx,
        })
    }

    pub fn run(mut self) -> Result<()> {
        tokio::spawn(async move {
            let mut buffer = [0u8; 2048];
            loop_select! {
                new_register = self.register_receiver.recv() => {
                    let new_register = new_register.unwrap();
                    if self.open_ports.iter().any(|open_port| new_register.port == open_port.port) {
                        panic!("port e tekrari");
                    }
                    self.open_ports.push(new_register);
                },
                len = self.socket.recv(&mut buffer) => {
                    let Ok(len) = len else {
                        continue
                    };
                    let Ok((data, sender)) = self.recv_from(&mut buffer, len) else {
                        continue
                    };
                    if let Err(_e) = sender.send(data).await {
                        todo!("remove port from open_ports")
                    }
                }
            }
        });
        Ok(())
    }

    fn recv_from(&self, buffer: &mut [u8], len: usize) -> Result<(OwnnedData, Sender<OwnnedData>)> {
        let (iph, icmp) = parse_icmpv4_packet(&buffer[..len])?;

        // ignore icmp reply packets, because many hosts send reply for
        // each icmp echo packet so we need to ignore that
        if icmp.type_u8() == 0 {
            return Err(ErrorKind::InvalidInput.into());
        }

        let bytes5to8 = icmp.bytes5to8();
        // icmp is on layer 3 so it has no idea about ports
        // we use identification part of icmp packet that usually
        // is the pid of ping program as destination port to identify
        // packets that are really meant for us
        let destination_port = u16::from_be_bytes([bytes5to8[0], bytes5to8[1]]);

        // we also use sequence part of icmp packet as source port
        let source_port = u16::from_be_bytes([bytes5to8[2], bytes5to8[3]]);

        // no port corresponding to dest port
        let Some(open_port) = self.open_ports.iter().find(|p| p.port == destination_port) else {
            return Err(ErrorKind::InvalidInput.into());
        };

        let source_addr = SocketAddrV4::new(iph.source_addr(), source_port);
        let payload_len = icmp.payload().len();

        let mut result = Vec::with_capacity(payload_len);
        unsafe { result.set_len(payload_len) };
        result[..payload_len].copy_from_slice(&buffer[len - payload_len..len]);

        let data = OwnnedData {
            packet: result,
            from_addr: source_addr,
        };
        Ok((data, open_port.sender.clone()))
    }
}

fn parse_icmpv4_packet(bytes: &[u8]) -> Result<(Ipv4HeaderSlice, Icmpv4Slice)> {
    let ip_header = Ipv4HeaderSlice::from_slice(bytes).map_err(|_| ErrorKind::InvalidData)?;
    let payload_index: usize = (ip_header.total_len() - ip_header.payload_len()).into();
    let icmp =
        Icmpv4Slice::from_slice(&bytes[payload_index..]).map_err(|_| ErrorKind::InvalidData)?;
    Ok((ip_header, icmp))
}
