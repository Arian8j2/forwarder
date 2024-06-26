use anyhow::bail;
use std::process::Stdio;
use std::{net::SocketAddrV4, str::FromStr, time::Duration};
use tokio::process::{Child, Command};
use tokio::task::JoinSet;
use tokio::time::timeout;

const HANDSHAKE_TASK_TIMEOUT: Duration = Duration::from_secs(3);

#[tokio::test(flavor = "multi_thread")]
async fn test_redirect_packets_via_udp() {
    // real client ---udp--> f1 ---udp--> f2 ----udp---> real server

    // capture childs because if child get dropped the process also
    // gets terminated to avoid zombie processes
    let _f1_child =
        spawn_forwarder("-l 127.0.0.1:10001/udp -r 127.0.0.1:30001/udp -p password").unwrap();
    let _f2_child =
        spawn_forwarder("-l 127.0.0.1:30001/udp -r 127.0.0.1:3939/udp -p password").unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let real_server_addr = SocketAddrV4::from_str("127.0.0.1:3939").unwrap();
    let connect_to = SocketAddrV4::from_str("127.0.0.1:10001").unwrap();
    test_udp_handshake(connect_to, real_server_addr).await;
}

fn spawn_forwarder(args: &str) -> anyhow::Result<Child> {
    let cmd_path = assert_cmd::cargo::cargo_bin("forwarder");
    if !cmd_path.is_file() {
        bail!(
            "Cannot find binary generated by cargo in '{}'",
            cmd_path.display()
        );
    }

    let mut cmd = Command::new(cmd_path);
    cmd.kill_on_drop(true);
    cmd.stdout(Stdio::null());
    let child = cmd.args(args.split(" ")).spawn()?;
    Ok(child)
}

#[ignore = "requires root access, beacuse it has to deal with raw socket"]
#[tokio::test(flavor = "multi_thread")]
async fn test_redirect_packets_via_icmp() {
    // real client ---udp--> f1 ---icmp----> f2 ----udp---> real server
    let _f1_child =
        spawn_forwarder("-l 127.0.0.1:10002/udp -r 127.0.0.1:30002/icmp -p password").unwrap();
    let _f2_child =
        spawn_forwarder("-l 127.0.0.1:30002/icmp -r 127.0.0.1:4040/udp -p password").unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let real_server_addr = SocketAddrV4::from_str("127.0.0.1:4040").unwrap();
    let connect_to = SocketAddrV4::from_str("127.0.0.1:10002").unwrap();
    test_udp_handshake(connect_to, real_server_addr).await;
}

#[ignore = "requires root access, beacuse it has to deal with raw socket"]
#[tokio::test(flavor = "multi_thread")]
async fn test_redirect_packets_via_icmp_v6() {
    // real client ---udp--> f1 ---icmp----> f2 ----udp---> real server
    let _f1_child =
        spawn_forwarder("-l 127.0.0.1:10020/udp -r [::1]:30012/icmp -p password").unwrap();
    let _f2_child =
        spawn_forwarder("-l [::1]:30012/icmp -r 127.0.0.1:4848/udp -p password").unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let real_server_addr = SocketAddrV4::from_str("127.0.0.1:4848").unwrap();
    let connect_to = SocketAddrV4::from_str("127.0.0.1:10020").unwrap();
    test_udp_handshake(connect_to, real_server_addr).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirect_packets_via_udp_ipv6() {
    let _f1_child =
        spawn_forwarder("-l 127.0.0.1:10003/udp -r [::1]:30003/udp -p password").unwrap();
    let _f2_child =
        spawn_forwarder("-l [::1]:30003/udp -r 127.0.0.1:4141/udp -p password").unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let real_server_addr = SocketAddrV4::from_str("127.0.0.1:4141").unwrap();
    let connect_to = SocketAddrV4::from_str("127.0.0.1:10003").unwrap();
    test_udp_handshake(connect_to, real_server_addr).await;
}

async fn test_udp_handshake(connect_to: SocketAddrV4, server_addr: SocketAddrV4) {
    use tokio::net::UdpSocket;

    let mut tasks = JoinSet::new();
    tasks.spawn(timeout(HANDSHAKE_TASK_TIMEOUT, async move {
        let server = UdpSocket::bind(server_addr).await?;
        let mut buf = vec![0u8; 2048];
        let (len, addr) = server.recv_from(&mut buf).await?;
        assert_eq!(&buf[..len], "syn".as_bytes());

        server.send_to("ack".as_bytes(), addr).await?;
        anyhow::Ok(())
    }));

    tasks.spawn(timeout(HANDSHAKE_TASK_TIMEOUT, async move {
        let client_addr = SocketAddrV4::from_str("127.0.0.1:0").unwrap();
        let client = UdpSocket::bind(client_addr).await?;
        client.connect(connect_to).await?;
        client.send("syn".as_bytes()).await?;

        let mut buf = vec![0u8; 2048];
        let len = client.recv(&mut buf).await?;
        assert_eq!(&buf[..len], "ack".as_bytes());
        anyhow::Ok(())
    }));

    while let Some(task) = tasks.join_next().await {
        let maybe_task_completed = task.unwrap();
        assert!(maybe_task_completed.is_ok(), "Handshake timed out");
        maybe_task_completed.unwrap().unwrap();
    }
}
