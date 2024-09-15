mod cli;
mod encryption;
mod peer;
mod server;
mod socket;

use anyhow::Result;
use clap::Parser;
use cli::Args;
use log::{info, LevelFilter};
use simple_logger::SimpleLogger;
use std::{env, str::FromStr};

fn main() -> Result<()> {
    let log_level = env::var("RUST_LOG").unwrap_or("info".to_owned());
    SimpleLogger::new()
        .with_level(LevelFilter::from_str(&log_level)?)
        .init()
        .unwrap();

    log_version();
    let cli = Args::parse();
    server::run_server(cli.listen_addr, cli.remote_addr, cli.passphrase)?;
    Ok(())
}

fn log_version() {
    info!(
        "latest commit: ({}, {})",
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_GIT_COMMIT_MESSAGE"),
    );
}
