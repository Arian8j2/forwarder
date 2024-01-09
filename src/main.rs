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
    use crate::socket::{SocketProtocol, SocketUri};
    use ntest::timeout;
    use std::{net::SocketAddrV4, str::FromStr};
    use tokio::task::JoinSet;

    #[tokio::test(flavor = "multi_thread")]
    #[timeout(4000)]
    async fn test_redirect_packets() {
        let redirect_addr = SocketAddrV4::from_str("0.0.0.0:9392").unwrap();
        let server_addr = SocketAddrV4::from_str("0.0.0.0:2292").unwrap();
        let second_forwarder_addr = SocketAddrV4::from_str("0.0.0.0:2392").unwrap();

        tokio::spawn(async move {
            let server_uri = SocketUri::new(server_addr.clone(), SocketProtocol::Udp);
            let mut server = Server::new(server_uri).await.unwrap();
            server.set_passphrase("password");
            server
                .run(SocketUri::new(second_forwarder_addr, SocketProtocol::Udp))
                .await;
        });

        tokio::spawn(async move {
            let server_uri = SocketUri::new(second_forwarder_addr.clone(), SocketProtocol::Udp);
            let mut server = Server::new(server_uri).await.unwrap();
            server.set_passphrase("password");
            server
                .run(SocketUri::new(redirect_addr, SocketProtocol::Udp))
                .await;
        });

        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
            // waits to receive 'salam' then it will respond with 'khobi?'
            let server = tokio::net::UdpSocket::bind(redirect_addr).await?;
            let mut buf = vec![0u8; 2048];
            let (len, addr) = server.recv_from(&mut buf).await?;
            assert_eq!(&buf[..len], "salam".as_bytes());

            server.send_to("khobi?".as_bytes(), addr).await?;
            anyhow::Ok(())
        });

        tasks.spawn(async move {
            // sends 'salam' then will wait to receive 'khobi?'
            let client = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
            client.connect(server_addr).await?;
            client.send("salam".as_bytes()).await?;

            let mut buf = vec![0u8; 2048];
            let len = client.recv(&mut buf).await?;
            assert_eq!(&buf[..len], "khobi?".as_bytes());
            anyhow::Ok(())
        });

        while let Some(task) = tasks.join_next().await {
            task.unwrap().unwrap();
        }
    }
}
