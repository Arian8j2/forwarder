use super::{setting::IcmpSetting, AsyncRawSocket};
use crate::macros::loop_select;
use crate::server::MAX_PACKET_SIZE;
use etherparse::{Icmpv4Slice, Ipv4HeaderSlice};
use log::{info, warn};
use socket2::{Domain, Protocol};
use std::{io::Result, net::Ipv4Addr, net::SocketAddrV4};
use tokio::sync::mpsc::{self, Receiver, Sender};

const MAX_PORT_LISTENERS_CHANNEL_QUEUE_SIZE: usize = 256;
const PORT_LISTENERS_BASE_CAPACITY: usize = 50;

pub struct PortListener {
    pub port: u16,
    pub sender: Sender<OwnnedData>,
}

#[derive(Debug)]
pub struct OwnnedData {
    pub from_addr: SocketAddrV4,
    pub packet: Vec<u8>,
}

/// Listens for icmp packets and send them to `PortListener`s that
/// registered their ports. At first created `PacketReceiver` because
/// of miscalculation on my mind and thought need it but really it's not
/// necessary, but kept it because it will reduce cpu usage compared to
/// other option that needed every `IcmpSocket` to listen for every *icmp*
/// packets and parse them and see if that packet actually is for them or not and ...
pub struct PacketReceiver {
    socket: AsyncRawSocket,
    open_ports: Vec<PortListener>,
    register_receiver: Receiver<PortListener>,
    setting: IcmpSetting,
}

impl PacketReceiver {
    /// Returns new `PacketReceiver` with a mpsc sender so
    /// `IcmpSocket` instances can use that sender to register
    /// their ports and receiver
    pub fn new(setting: IcmpSetting) -> Result<(Self, Sender<PortListener>)> {
        let socket = AsyncRawSocket::new(Domain::IPV4, Protocol::ICMPV4)?;
        let adress = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0);
        socket.bind(&adress.into())?;

        let (tx, rx) = mpsc::channel::<PortListener>(MAX_PORT_LISTENERS_CHANNEL_QUEUE_SIZE);
        info!("new icmp packet receiver");

        Ok((
            PacketReceiver {
                socket,
                open_ports: Vec::with_capacity(PORT_LISTENERS_BASE_CAPACITY),
                register_receiver: rx,
                setting,
            },
            tx,
        ))
    }

    pub fn run(mut self) -> Result<()> {
        tokio::spawn(async move {
            let mut buffer = [0u8; MAX_PACKET_SIZE];
            loop_select! {
                new_register = self.register_receiver.recv() => {
                    let new_register = new_register.unwrap();
                    if self.open_ports.iter().any(|open_port| new_register.port == open_port.port) {
                        panic!("register duplicate port");
                    }
                    self.open_ports.push(new_register);
                },
                maybe_len = self.socket.recv(&mut buffer) => {
                    let Ok(len) = maybe_len else {
                        continue
                    };
                    let Some((data, port_listener)) = self.handle_packet(&mut buffer, len) else {
                        continue
                    };
                    if let Err(_e) = port_listener.sender.send(data).await {
                        let index = self.open_ports
                            .iter()
                            .position(|open_port| open_port.port == port_listener.port)
                            .unwrap();
                        self.open_ports.remove(index);
                    }
                }
            }
        });
        Ok(())
    }

    fn handle_packet(&self, buffer: &mut [u8], len: usize) -> Option<(OwnnedData, &PortListener)> {
        let (iph, icmp) = Self::parse_icmpv4_packet(&buffer[..len])?;
        self.validate_icmp_packet(&icmp);

        let bytes5to8 = icmp.bytes5to8();
        // icmp is on layer 3 so it has no idea about ports
        // we use identification part of icmp packet that usually
        // is the pid of ping program as destination port to identify
        // packets that are really meant for us
        let destination_port = u16::from_be_bytes([bytes5to8[0], bytes5to8[1]]);

        // we also use sequence part of icmp packet as source port
        let source_port = u16::from_be_bytes([bytes5to8[2], bytes5to8[3]]);

        // no port corresponding to dest port
        let Some(port_listener) = self.open_ports.iter().find(|p| p.port == destination_port)
        else {
            return None;
        };

        let source_addr = SocketAddrV4::new(iph.source_addr(), source_port);
        let payload_len = icmp.payload().len();

        let result = buffer[len - payload_len..len].to_vec();
        let data = OwnnedData {
            packet: result,
            from_addr: source_addr,
        };
        Some((data, port_listener))
    }

    fn validate_icmp_packet(&self, icmp: &Icmpv4Slice) -> Option<()> {
        let icmp_type = icmp.type_u8();
        if icmp_type != self.setting.icmp_type {
            warn!("received icmp packet with unexpected type field {icmp_type}");
            return None;
        }

        let icmp_code = icmp.code_u8();
        if icmp_code != self.setting.code {
            warn!("received icmp packet with unexpected code field {icmp_code}");
            return None;
        }
        Some(())
    }

    fn parse_icmpv4_packet(bytes: &[u8]) -> Option<(Ipv4HeaderSlice, Icmpv4Slice)> {
        let ip_header = Ipv4HeaderSlice::from_slice(bytes).ok()?;
        let payload_index: usize = (ip_header.total_len() - ip_header.payload_len()).into();
        let icmp = Icmpv4Slice::from_slice(&bytes[payload_index..]).ok()?;
        Some((ip_header, icmp))
    }
}
