mod cli;
mod client;
mod encryption;
mod macros;
mod server;
mod socket;

use anyhow::Result;
use clap::Parser;
use cli::Args;
use log::{info, LevelFilter};
use server::Server;
use simple_logger::SimpleLogger;
use socket::IcmpSettingSetter;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    log_version();
    let cli = Args::parse();
    cli.set_icmp_setting()?;

    let mut server = Server::new(cli.listen_addr).await?;
    if let Some(passphrase) = cli.passphrase {
        server.set_passphrase(&passphrase);
    }

    server.run(cli.remote_addr).await;
    Ok(())
}

fn log_version() {
    info!(
        "latest commit: ({}, {})",
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_GIT_COMMIT_MESSAGE"),
    );
}
