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
use std::{env, str::FromStr};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let log_level = env::var("RUST_LOG").unwrap_or("info".to_owned());
    SimpleLogger::new()
        .with_level(LevelFilter::from_str(&log_level)?)
        .init()
        .unwrap();

    log_version();
    let cli = Args::parse();
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
