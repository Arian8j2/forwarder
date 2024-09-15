use forwarder::{server::run_server, socket::SocketUri};
use std::{io::ErrorKind, net::UdpSocket, str::FromStr, time::Duration};

#[test]
fn test_udp_forwarder() {
    let forwarder_uri = SocketUri::from_str("127.0.0.1:38801/udp").unwrap();
    let remote_uri = SocketUri::from_str("127.0.0.1:38802/udp").unwrap();

    std::thread::spawn(move || {
        run_server(forwarder_uri, remote_uri, None).unwrap();
    });

    let remote = UdpSocket::bind(remote_uri.addr).unwrap();
    remote
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let remote_thread = std::thread::spawn(move || {
        let mut buffer = [0u8; 100];
        match remote.recv(&mut buffer) {
            Ok(size) => Ok(buffer[..size].to_vec()),
            Err(error) => {
                if error.kind() == ErrorKind::TimedOut {
                    Err(String::from("remote not received the packet"))
                } else {
                    Err(format!("recv error: {error}"))
                }
            }
        }
    });

    std::thread::sleep(Duration::from_millis(100));
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    client.connect(forwarder_uri.addr).unwrap();
    client.send("hello".as_bytes()).unwrap();
    let remote_result = remote_thread.join().unwrap().unwrap();
    assert_eq!(remote_result, "hello".as_bytes());
}

#[test]
fn test_udp_double_forwarder() {
    let forwarder_uri = SocketUri::from_str("127.0.0.1:38803/udp").unwrap();
    let second_forwarder_uri = SocketUri::from_str("127.0.0.1:38804/udp").unwrap();
    let remote_uri = SocketUri::from_str("127.0.0.1:38805/udp").unwrap();

    std::thread::spawn(move || {
        run_server(
            forwarder_uri,
            second_forwarder_uri,
            Some(String::from("some_password")),
        )
        .unwrap();
    });
    std::thread::spawn(move || {
        run_server(
            second_forwarder_uri,
            remote_uri,
            Some(String::from("some_password")),
        )
        .unwrap();
    });

    let remote = UdpSocket::bind(remote_uri.addr).unwrap();
    remote
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let remote_thread = std::thread::spawn(move || {
        let mut buffer = [0u8; 100];
        match remote.recv(&mut buffer) {
            Ok(size) => Ok(buffer[..size].to_vec()),
            Err(error) => {
                if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut {
                    Err(String::from("remote not received the packet"))
                } else {
                    Err(format!("recv error: {error}"))
                }
            }
        }
    });

    // wait for remote thread to start listening
    std::thread::sleep(Duration::from_millis(200));

    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    client.connect(forwarder_uri.addr).unwrap();
    client.send("hello".as_bytes()).unwrap();
    let remote_result = remote_thread.join().unwrap().unwrap();
    assert_eq!(remote_result, "hello".as_bytes());
}

#[test]
fn test_udp_double_forwarder_back_and_forth() {
    let forwarder_uri = SocketUri::from_str("127.0.0.1:38806/udp").unwrap();
    let second_forwarder_uri = SocketUri::from_str("127.0.0.1:38807/udp").unwrap();
    let remote_uri = SocketUri::from_str("127.0.0.1:38808/udp").unwrap();

    std::thread::spawn(move || {
        run_server(
            forwarder_uri,
            second_forwarder_uri,
            Some(String::from("some_password")),
        )
        .unwrap();
    });
    std::thread::spawn(move || {
        run_server(
            second_forwarder_uri,
            remote_uri,
            Some(String::from("some_password")),
        )
        .unwrap();
    });

    let remote = UdpSocket::bind(remote_uri.addr).unwrap();
    remote
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 100];
        let (size, from_addr) = remote.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..size], "hello".as_bytes());
        remote.send_to("hi".as_bytes(), from_addr).unwrap();
    });

    // wait for remote thread to start listening
    std::thread::sleep(Duration::from_millis(200));

    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    client.connect(forwarder_uri.addr).unwrap();
    client.send("hello".as_bytes()).unwrap();
    let mut buffer = [0u8; 100];
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let size = client.recv(&mut buffer).unwrap();
    assert_eq!(&buffer[..size], "hi".as_bytes());
}
