use anyhow::Context;
use forwarder::uri::{Protocol, Uri};
use socket2::{Domain, Type};
use std::{
    mem::MaybeUninit,
    net::{SocketAddr, UdpSocket},
    os::fd::{AsRawFd, FromRawFd},
    str::FromStr,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

/// Number of clients that simontensly sends packets to forwarder
const CLIENTS_COUNT: usize = 150;

/// Client packet per second
const CLIENT_PPS: usize = 400;

/// Size of packet in bytes
const PACKET_SIZE: usize = 750;

/// Thread counts that handle server packets
const SERVER_THREAD_COUNT: usize = 5;

/// Duration that one round of benchmark takes
const BENCHMARK_DURATION: Duration = Duration::from_secs(10);

fn main() -> anyhow::Result<()> {
    let args = std::env::args();
    let protocol = if args.len() == 2 {
        let protocol_name = args.last().unwrap();
        Protocol::from_str(&protocol_name)
            .with_context(|| format!("cannot parse protocol name '{protocol_name}'"))?
    } else {
        Protocol::Udp
    };

    let forwarder_uri = Uri::from_str("127.0.0.1:38701/udp")?;
    let second_forwarder_uri = Uri {
        addr: "127.0.0.1:38702".parse()?,
        protocol,
    };
    let remote_uri = Uri::from_str("127.0.0.1:38703/udp")?;

    std::thread::spawn(move || {
        forwarder::run(forwarder_uri, second_forwarder_uri, None).unwrap();
    });
    std::thread::spawn(move || {
        forwarder::run(second_forwarder_uri, remote_uri, None).unwrap();
    });

    let remote_received_packet_count = Arc::new(AtomicU32::new(0));
    for _ in 0..SERVER_THREAD_COUNT {
        let remote_received_packet_count = remote_received_packet_count.clone();
        std::thread::spawn(move || {
            server_thread(remote_uri.addr, remote_received_packet_count);
        });
    }

    let client_sent_packet_count = Arc::new(AtomicU32::new(0));
    let client_received_packet_count = Arc::new(AtomicU32::new(0));
    for _ in 0..CLIENTS_COUNT {
        let forwarder_addr = forwarder_uri.addr;
        let client_packet_count = client_sent_packet_count.clone();
        let client_received_packet_count = client_received_packet_count.clone();
        std::thread::spawn(move || {
            client_thread(
                forwarder_addr,
                client_packet_count,
                client_received_packet_count,
            )
        });
    }

    println!("benchmarking {protocol} protocol...");
    std::thread::sleep(BENCHMARK_DURATION);
    let client_sent_pc = client_sent_packet_count.load(Ordering::Relaxed);
    let client_received_pc = client_received_packet_count.load(Ordering::Relaxed);
    println!("packets client sent: {client_sent_pc}");
    println!("packets client received: {client_received_pc}");
    let diff = client_sent_pc.abs_diff(client_received_pc);
    let packet_lost = diff as f32 * 100.0 / client_sent_pc as f32;
    println!("\ndiff: {diff}");
    println!("packet lost: {packet_lost:.3}%");

    Ok(())
}

/// runs a echo server that listens on address `remote_addr` and also
/// increases the `remote_received_packet_count` on each packet that receives
fn server_thread(remote_addr: SocketAddr, remote_received_packet_count: Arc<AtomicU32>) {
    let socket =
        socket2::Socket::new(Domain::IPV4, Type::DGRAM, Some(socket2::Protocol::UDP)).unwrap();
    socket.set_reuse_port(true).unwrap();
    socket.bind(&remote_addr.into()).unwrap();

    let mut buffer = [0u8; PACKET_SIZE];
    loop {
        let (_, from_addr) = socket
            .recv_from(unsafe { &mut *(&mut buffer as *mut [u8] as *mut [MaybeUninit<u8>]) })
            .unwrap();
        remote_received_packet_count.fetch_add(1, Ordering::Relaxed);
        socket.send_to(&buffer, &from_addr).unwrap();
    }
}

/// tries to send packet to `forwarder_addr` based on `CLIENT_PPS`
/// and increase `client_packet_count` on each packet that sends
/// and increase `client_received_packet_count` on each packet that receives
fn client_thread(
    forwarder_addr: SocketAddr,
    client_packet_count: Arc<AtomicU32>,
    client_received_packet_count: Arc<AtomicU32>,
) {
    let buffer = [0u8; PACKET_SIZE];
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.connect(forwarder_addr).unwrap();

    let socket_clone = unsafe { UdpSocket::from_raw_fd(socket.as_raw_fd()) };
    std::thread::spawn(move || {
        let mut buffer = [0u8; PACKET_SIZE];
        loop {
            socket_clone.recv(&mut buffer).unwrap();
            client_received_packet_count.fetch_add(1, Ordering::Relaxed);
        }
    });

    let sleep_time = Duration::from_micros((1_000_000 / CLIENT_PPS).try_into().unwrap());
    loop {
        socket.send(&buffer).unwrap();
        client_packet_count.fetch_add(1, Ordering::Relaxed);
        std::thread::sleep(sleep_time);
    }
}
