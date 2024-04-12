use super::ether_helper::IcmpSlice;
use super::{AsyncRawSocket, IcmpSocket, RegisterMsg};
use crate::{macros::loop_select, server::MAX_PACKET_SIZE};
use etherparse::Ipv4HeaderSlice;
use log::{debug, info};
use socket2::SockAddr;
use std::{io::Result, net::SocketAddr};
use tokio::sync::mpsc::{self, Receiver, Sender};

const MAX_PORT_LISTENERS_CHANNEL_QUEUE_SIZE: usize = 256;
const PORT_LISTENERS_BASE_CAPACITY: usize = 50;

pub struct PortListener {
    pub port: u16,
    pub sender: Sender<OwnnedData>,
}

#[derive(Debug)]
pub struct OwnnedData {
    pub from_addr: SocketAddr,
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
    receiver: Receiver<RegisterMsg>,
    is_ipv6: bool,
    correct_icmp_type: u8,
}

impl PacketReceiver {
    /// Returns new `PacketReceiver` with a mpsc sender so
    /// `IcmpSocket` instances can use that sender to register
    /// their ports and receiver
    pub fn new(address: SocketAddr) -> Result<(Self, Sender<RegisterMsg>)> {
        let is_ipv6 = address.is_ipv6();
        let socket = IcmpSocket::bind_socket(address)?;
        let (tx, rx) = mpsc::channel::<RegisterMsg>(MAX_PORT_LISTENERS_CHANNEL_QUEUE_SIZE);
        info!("new icmp packet receiver");

        let correct_icmp_type = if is_ipv6 {
            etherparse::icmpv6::TYPE_ECHO_REQUEST
        } else {
            etherparse::icmpv4::TYPE_ECHO_REQUEST
        };

        Ok((
            PacketReceiver {
                socket,
                open_ports: Vec::with_capacity(PORT_LISTENERS_BASE_CAPACITY),
                receiver: rx,
                is_ipv6,
                correct_icmp_type,
            },
            tx,
        ))
    }

    #[inline]
    fn search_listener_port(&self, port: &u16) -> std::prelude::v1::Result<usize, usize> {
        self.open_ports.binary_search_by_key(port, |p| p.port)
    }

    pub fn run(mut self) -> Result<()> {
        tokio::spawn(async move {
            let mut buffer = [0u8; MAX_PACKET_SIZE];
            loop_select! {
                message = self.receiver.recv() => {
                    self.handle_port_registration_messages(message.unwrap());
                },
                Ok((len, from_addr)) = self.socket.recv_from(&mut buffer) => {
                    let Some((data, port_listener)) = self.handle_packet(&mut buffer, len, from_addr) else {
                        continue
                    };
                    if let Err(_e) = port_listener.sender.send(data).await {
                        let index = self.search_listener_port(&port_listener.port).unwrap();
                        self.open_ports.remove(index);
                    }
                }
            }
        });
        Ok(())
    }

    // each `IcmpSocket` instance register and unregister their ports using channel
    #[inline]
    fn handle_port_registration_messages(&mut self, message: RegisterMsg) {
        match message {
            RegisterMsg::Register(new_register) => {
                let index = self.search_listener_port(&new_register.port).unwrap_err();
                self.open_ports.insert(index, new_register);
            }
            RegisterMsg::UnRegister { port } => {
                let index = self.search_listener_port(&port).unwrap();
                self.open_ports.remove(index);
            }
        }
    }

    fn handle_packet(
        &self,
        buffer: &mut [u8],
        len: usize,
        from_addr: SockAddr,
    ) -> Option<(OwnnedData, &PortListener)> {
        let icmp = self.parse_icmp_packet(&buffer[..len])?;
        self.validate_icmp_packet(&icmp)?;

        let bytes5to8 = icmp.bytes5to8();
        // icmp is on layer 3 so it has no idea about ports
        // we use identification part of icmp packet that usually
        // is the pid of ping program as destination port to identify
        // packets that are really meant for us
        let destination_port = u16::from_be_bytes([bytes5to8[0], bytes5to8[1]]);

        // we also use sequence part of icmp packet as source port
        let source_port = u16::from_be_bytes([bytes5to8[2], bytes5to8[3]]);

        // no port corresponding to dest port
        let Ok(port_listener_index) = self.search_listener_port(&destination_port) else {
            return None;
        };

        let mut source_addr = from_addr.as_socket().unwrap();
        source_addr.set_port(source_port);
        let payload_len = icmp.payload().len();

        let result = buffer[len - payload_len..len].to_vec();
        let data = OwnnedData {
            packet: result,
            from_addr: source_addr,
        };
        Some((data, &self.open_ports[port_listener_index]))
    }

    #[inline]
    fn validate_icmp_packet(&self, icmp: &IcmpSlice) -> Option<()> {
        let icmp_type = icmp.type_u8();
        if icmp_type != self.correct_icmp_type {
            debug!("unexpected icmp type {icmp_type}");
            return None;
        }

        let icmp_code = icmp.code_u8();
        if icmp_code != 0 {
            debug!("unexpected icmp code {icmp_code}");
            return None;
        }
        Some(())
    }

    #[inline]
    fn parse_icmp_packet<'a>(&self, bytes: &'a [u8]) -> Option<IcmpSlice<'a>> {
        // according to 'icmp6' man page on freebsd (seems like linux does this the same way):
        // 'Incoming packets on the socket are received with the IPv6 header and any extension headers removed'
        //
        // but on 'icmp' man page that is for icmpv4, it says:
        // 'Incoming packets are received with the IP header and options intact.'
        //
        // so we need to parse header in icmpv4 but not in icmpv6
        // why tf??? i don't know, and don't ask me how i found this out
        let payload_start_index = if self.is_ipv6 {
            0
        } else {
            let ip_header = Ipv4HeaderSlice::from_slice(bytes).ok()?;
            let payload_len: usize = ip_header.payload_len().into();
            bytes.len() - payload_len
        };

        let icmp = IcmpSlice::from_slice(self.is_ipv6, &bytes[payload_start_index..])?;
        Some(icmp)
    }
}
