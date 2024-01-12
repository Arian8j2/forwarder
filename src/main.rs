mod cli;
mod client;
mod encryption;
mod macros;
mod server;
mod socket;

use anyhow::Result;
use clap::Parser;
use cli::Args;
use log::LevelFilter;
use server::Server;
use simple_logger::SimpleLogger;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let cli = Args::parse();
    let mut server = Server::new(cli.listen_addr).await?;
    if let Some(passphrase) = cli.passphrase {
        server.set_passphrase(&passphrase);
    }

    server.run(cli.remote_addr).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socket::SocketUri;
    use ntest::timeout;
    use std::{net::SocketAddrV4, str::FromStr};
    use tokio::task::JoinSet;

    #[tokio::test(flavor = "multi_thread")]
    #[timeout(1000)]
    async fn test_redirect_packets_via_udp() {
        // real client ---udp--> f1 ---udp--> f2 ----udp---> real server
        spawn_forwarder_task(
            "127.0.0.1:10001/udp".try_into().unwrap(),
            "127.0.0.1:30001/udp".try_into().unwrap(),
            Some("password".to_owned()),
        )
        .await;

        spawn_forwarder_task(
            "127.0.0.1:30001/udp".try_into().unwrap(),
            "127.0.0.1:3939/udp".try_into().unwrap(),
            Some("password".to_owned()),
        )
        .await;

        let real_server_addr = SocketAddrV4::from_str("127.0.0.1:3939").unwrap();
        let connect_to = SocketAddrV4::from_str("127.0.0.1:10001").unwrap();
        test_udp_handshake(connect_to, real_server_addr).await;
    }

    #[ignore = "requires root access, beacuse it has to deal with raw socket"]
    #[tokio::test(flavor = "multi_thread")]
    #[timeout(1000)]
    async fn test_redirect_packets_via_icmp() {
        // real client ---udp--> f1 ---icmp----> f2 ----udp---> real server
        spawn_forwarder_task(
            "127.0.0.1:10002/udp".try_into().unwrap(),
            "127.0.0.1:30002/icmp".try_into().unwrap(),
            Some("password".to_owned()),
        )
        .await;

        spawn_forwarder_task(
            "127.0.0.1:30002/icmp".try_into().unwrap(),
            "127.0.0.1:4040/udp".try_into().unwrap(),
            Some("password".to_owned()),
        )
        .await;

        let real_server_addr = SocketAddrV4::from_str("127.0.0.1:4040").unwrap();
        let connect_to = SocketAddrV4::from_str("127.0.0.1:10002").unwrap();
        test_udp_handshake(connect_to, real_server_addr).await;
    }

    async fn spawn_forwarder_task(
        listen_uri: SocketUri,
        redirect_uri: SocketUri,
        password: Option<String>,
    ) {
        tokio::spawn(async move {
            let server_uri = SocketUri::new(listen_uri.addr, listen_uri.protocol);
            let mut server = Server::new(server_uri).await.unwrap();

            if let Some(pass) = password {
                server.set_passphrase(&pass);
            }

            server
                .run(SocketUri::new(redirect_uri.addr, redirect_uri.protocol))
                .await;
        });
    }

    async fn test_udp_handshake(connect_to: SocketAddrV4, server_addr: SocketAddrV4) {
        use tokio::net::UdpSocket;

        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
            let server = UdpSocket::bind(server_addr).await?;
            let mut buf = vec![0u8; 2048];
            let (len, addr) = server.recv_from(&mut buf).await?;
            assert_eq!(&buf[..len], "syn".as_bytes());

            server.send_to("ack".as_bytes(), addr).await?;
            anyhow::Ok(())
        });

        tasks.spawn(async move {
            let client_addr = SocketAddrV4::from_str("127.0.0.1:0").unwrap();
            let client = UdpSocket::bind(client_addr).await?;
            client.connect(connect_to).await?;
            client.send("syn".as_bytes()).await?;

            let mut buf = vec![0u8; 2048];
            let len = client.recv(&mut buf).await?;
            assert_eq!(&buf[..len], "ack".as_bytes());
            anyhow::Ok(())
        });

        while let Some(task) = tasks.join_next().await {
            task.unwrap().unwrap();
        }
    }
}
