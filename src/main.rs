mod cli;
mod client;
mod server;

use anyhow::Result;
use clap::Parser;
use cli::Args;
use log::LevelFilter;
use server::run_server;
use simple_logger::SimpleLogger;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let cli = Args::parse();
    run_server(cli.listen_addr.into(), cli.remote_addr.into()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntest::timeout;
    use std::{net::SocketAddr, str::FromStr, time::Duration};
    use tokio::time::sleep;

    #[tokio::test(flavor = "multi_thread")]
    #[timeout(4000)]
    async fn test_redirect_packets() {
        let redirect_addr = SocketAddr::from_str("0.0.0.0:9392").unwrap();
        let server_addr = SocketAddr::from_str("0.0.0.0:2292").unwrap();
        tokio::spawn(run_server(server_addr, redirect_addr));

        let redirect_thread = tokio::spawn(async move {
            let server = tokio::net::UdpSocket::bind(redirect_addr).await?;
            let mut buf = vec![0u8; 2048];

            let len = server.recv(&mut buf).await?;
            assert_eq!(buf[..len], vec![1, 2, 3, 4]);
            anyhow::Ok(())
        });

        tokio::spawn(async move {
            sleep(Duration::from_millis(300)).await;
            let client = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
            client.connect(server_addr).await?;
            client.send(&vec![1, 2, 3, 4]).await?;
            anyhow::Ok(())
        });

        redirect_thread.await.unwrap().unwrap();
        // client_mock_thread.await.unwrap().unwrap();
    }
}
