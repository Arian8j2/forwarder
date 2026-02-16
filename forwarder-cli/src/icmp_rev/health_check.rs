use crate::Args;
use forwarder::{
    create_socket_buffer,
    socket::{
        icmp::{create_bfp_filter, IcmpEchoType, IcmpSocket},
        SocketTrait,
    },
};
use std::{
    collections::VecDeque,
    io::ErrorKind,
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const HELLO_TIMEOUT: Duration = Duration::from_millis(500);

// health check is both keep alive to keep conntrack of firewall open and also a health
// check so if the firewall cache got reset we can renegotiate
const HEALTH_CHECK_MESSAGE: &[u8] = b"h";
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_millis(500);
const HEALTH_CHECK_PACKET_LOST_PERCENT: usize = 20;

/// amount of packets that we test for calculating packet lost
const PACKET_COUNTER_LEN: usize = 100;

pub fn initiate_server(cli: &Args) -> anyhow::Result<Arc<AtomicBool>> {
    let listen_addr = SocketAddr::new(cli.listen_uri.addr.ip(), cli.remote_uri.addr.port());
    let mut socket = IcmpSocket::bind(&listen_addr, IcmpEchoType::Request, false)?;
    socket.set_read_timeout(Some(HELLO_TIMEOUT))?;

    log::info!("waiting for client handshake...");
    let mut expected_message = b"syn";
    let buffer = create_socket_buffer!(10);
    loop {
        let Ok((size, addr)) = socket.recv_from(buffer) else {
            expected_message = b"syn";
            continue;
        };
        if addr != cli.remote_uri.addr {
            continue;
        }

        let message = &buffer[..size];
        if message != expected_message {
            expected_message = b"syn";
            continue;
        }

        if message == b"syn" {
            buffer[..3].copy_from_slice(b"ack");
            socket.send_to(&mut buffer[..3], &addr).ok();
            log::info!("received syn, sending ack, and waiting for syn ack");
            expected_message = b"sck";
        } else {
            log::info!("handshake completed, starting forwarder instances!");
            break;
        }
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_server_health_check(socket, shutdown.clone())?;
    Ok(shutdown)
}

fn spawn_server_health_check(
    mut socket: IcmpSocket,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    socket.set_read_timeout(Some(HEALTH_CHECK_INTERVAL))?;
    std::thread::spawn(move || {
        let buffer = create_socket_buffer!(1);
        let mut packet_counter = PacketCounter::new(PACKET_COUNTER_LEN);

        while !shutdown.load(Ordering::Relaxed) {
            let packet_lost = packet_counter.packet_lost();
            if packet_lost > HEALTH_CHECK_PACKET_LOST_PERCENT {
                log::info!("so many packet lost {packet_lost}%, lets reinitiate");
                shutdown.store(true, Ordering::Relaxed);
                break;
            }
            let sent = Instant::now();
            let res = socket
                .recv_from(buffer)
                .ok()
                .filter(|(size, _)| &buffer[..*size] == HEALTH_CHECK_MESSAGE);
            if let Some((size, addr)) = res {
                socket.send_to(&mut buffer[..size], &addr).ok();
            }
            packet_counter.add(res.is_some());
            if let Some(delay) = HEALTH_CHECK_INTERVAL.checked_sub(sent.elapsed()) {
                std::thread::sleep(delay);
            }
        }
    });
    Ok(())
}

pub fn initiate_client(cli: &Args, reverse_addr: IpAddr) -> anyhow::Result<Arc<AtomicBool>> {
    let mut socket = IcmpSocket::bind(&cli.listen_uri.addr, IcmpEchoType::Reply, false)?;
    socket.set_read_timeout(Some(HELLO_TIMEOUT))?;
    socket.inner_socket().attach_filter(&create_bfp_filter(
        false,
        IcmpEchoType::Reply,
        cli.listen_uri.addr.port(),
    ))?;

    let buffer = create_socket_buffer!(10);
    let to = SocketAddr::new(reverse_addr, cli.listen_uri.addr.port());

    loop {
        log::info!("sending handshake to server...");

        // syn
        buffer[..3].copy_from_slice(b"syn");
        socket.send_to(&mut buffer[..3], &to)?;

        match socket.recv_from(buffer) {
            Ok((size, addr)) => {
                if &buffer[..size] == b"ack" {
                    // syn ack
                    buffer[..3].copy_from_slice(b"sck");
                    socket.send_to(&mut buffer[..3], &to)?;
                    if addr == to {
                        log::info!("handshake done, lets go");
                        break;
                    }
                }
            }
            Err(error) => {
                if error.kind() != ErrorKind::WouldBlock {
                    return Err(error.into());
                }
            }
        }

        log::info!(
            "no ack from server, sleeping for {}s ...",
            cli.handshake_delay
        );
        std::thread::sleep(Duration::from_secs(cli.handshake_delay.into()));
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_client_health_check(socket, to, shutdown.clone())?;
    Ok(shutdown)
}

fn spawn_client_health_check(
    mut socket: IcmpSocket,
    server_addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    socket.set_read_timeout(Some(HEALTH_CHECK_INTERVAL))?;
    std::thread::spawn(move || {
        let buffer = create_socket_buffer!(HEALTH_CHECK_MESSAGE.len());
        let mut packet_counter = PacketCounter::new(PACKET_COUNTER_LEN);

        while !shutdown.load(Ordering::Relaxed) {
            let packet_lost = packet_counter.packet_lost();
            if packet_lost > HEALTH_CHECK_PACKET_LOST_PERCENT {
                log::info!("so many packet lost {packet_lost}%, lets reinitiate");
                shutdown.store(true, Ordering::Relaxed);
                break;
            }
            let sent = Instant::now();
            buffer.copy_from_slice(HEALTH_CHECK_MESSAGE);
            socket.send_to(buffer, &server_addr).ok();
            let is_ok = socket
                .recv_from(buffer)
                .is_ok_and(|(size, _)| &buffer[0..size] == HEALTH_CHECK_MESSAGE);
            packet_counter.add(is_ok);
            if let Some(delay) = HEALTH_CHECK_INTERVAL.checked_sub(sent.elapsed()) {
                std::thread::sleep(delay);
            }
        }
    });
    Ok(())
}

struct PacketCounter(VecDeque<bool>);

impl PacketCounter {
    fn new(size: usize) -> Self {
        let mut packets = VecDeque::new();
        // i don't want premature high packet lost
        packets.extend(std::iter::repeat_n(true, size));
        Self(packets)
    }

    fn add(&mut self, is_ok: bool) {
        self.0.pop_front();
        self.0.push_back(is_ok);
    }

    fn packet_lost(&self) -> usize {
        self.0.iter().filter(|&packet| !packet).count() * 100 / self.0.len()
    }
}
