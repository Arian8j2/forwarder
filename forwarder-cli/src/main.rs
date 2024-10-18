use anyhow::Context;
use clap::Parser;
use log::{info, LevelFilter};
use simple_logger::SimpleLogger;
use std::{env, str::FromStr};

/// Lightweight UDP forwarder and UDP over ICMP
#[derive(Parser)]
#[command(about)]
pub struct Args {
    /// Address and protocol that forwarder will listen on
    #[arg(short, long)]
    pub listen_uri: forwarder::uri::Uri,

    /// Address and protocol of remote server that forwarder will forward to
    #[arg(short, long)]
    pub remote_uri: forwarder::uri::Uri,

    /// The packets will get encrypted/decrypted by this passphrase
    #[arg(short, long)]
    pub passphrase: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Args::parse();
    setup_logger().with_context(|| "couldn't setup logger")?;
    log_version();
    forwarder::run(cli.listen_uri, cli.remote_uri, cli.passphrase)?;
    Ok(())
}

fn setup_logger() -> anyhow::Result<()> {
    let log_level = match env::var("RUST_LOG") {
        Ok(var) => LevelFilter::from_str(&var)?,
        Err(_) => LevelFilter::Info,
    };
    SimpleLogger::new().with_level(log_level).init()?;
    Ok(())
}

fn log_version() {
    info!(
        "latest commit: ({}, {})",
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_GIT_COMMIT_MESSAGE"),
    );
}
