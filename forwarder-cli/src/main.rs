use anyhow::Result;
use clap::Parser;
use log::{info, LevelFilter};
use simple_logger::SimpleLogger;
use std::{env, str::FromStr};

/// Simple program to forward udp packets
#[derive(Parser)]
#[command(about)]
pub struct Args {
    #[arg(short, long)]
    pub listen_addr: forwarder::socket::SocketUri,

    #[arg(short, long)]
    pub remote_addr: forwarder::socket::SocketUri,

    #[arg(
        short,
        long,
        help = "The packets will get encrypted/decrypted by this passphrase"
    )]
    pub passphrase: Option<String>,
}

fn main() -> Result<()> {
    let log_level = env::var("RUST_LOG").unwrap_or("info".to_owned());
    SimpleLogger::new()
        .with_level(LevelFilter::from_str(&log_level)?)
        .init()
        .unwrap();

    log_version();
    let cli = Args::parse();
    forwarder::run_server(cli.listen_addr, cli.remote_addr, cli.passphrase);
    Ok(())
}

fn log_version() {
    info!(
        "latest commit: ({}, {})",
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_GIT_COMMIT_MESSAGE"),
    );
}
