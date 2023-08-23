mod client;
mod server;

use anyhow::Result;
use log::LevelFilter;
use server::run_server;
use simple_logger::SimpleLogger;

const LISTEN_ADDR: &str = "0.0.0.0:3939";
const REDIRECT_ADDR: &str = "0.0.0.0:8585";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();
    run_server(LISTEN_ADDR, REDIRECT_ADDR).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntest::timeout;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(flavor = "multi_thread")]
    #[timeout(4000)]
    async fn test_redirect_packets() {
        let redirect_addr = "0.0.0.0:9392";
        let server_addr = "0.0.0.0:2292";
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
