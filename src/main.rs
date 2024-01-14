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
use socket::IcmpSettingSetter;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let cli = Args::parse();
    cli.set_icmp_setting()?;

    let mut server = Server::new(cli.listen_addr).await?;
    if let Some(passphrase) = cli.passphrase {
        server.set_passphrase(&passphrase);
    }

    server.run(cli.remote_addr).await;
    Ok(())
}
