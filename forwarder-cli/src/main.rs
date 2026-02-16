use anyhow::{bail, ensure, Context};
use clap::Parser;
use forwarder::uri::{Protocol, Uri};
use log::{info, LevelFilter};
use simple_logger::SimpleLogger;
use std::{
    env, net::IpAddr, process::Command, str::FromStr, sync::atomic::Ordering, time::Duration,
};

mod health_check;
mod pushack_firewall;

/// Lightweight UDP forwarder and UDP over ICMP
#[derive(Parser)]
#[command(about)]
pub struct Args {
    /// Address and protocol that forwarder will listen on
    #[arg(short, long)]
    pub listen_uri: Uri,

    /// Address and protocol of remote server that forwarder will forward to
    #[arg(short, long)]
    pub remote_uri: Uri,

    /// The packets will get encrypted/decrypted by this passphrase
    #[arg(short, long)]
    pub passphrase: Option<String>,

    /// When having one icmp uri this will reverse the icmp request types
    #[arg(short = 'R', long)]
    pub reverse: bool,

    /// Ip of server you wanna connect to and start the icmp connection
    #[arg(long)]
    pub reverse_addr: Option<IpAddr>,

    /// Set automatically by parent to specify child
    #[arg(long, hide = true)]
    pub child: bool,

    /// Amount of seconds to wait when icmp handshake failed in reverse mode
    #[arg(long, default_value = "2")]
    pub handshake_delay: u32,
}

fn main() -> anyhow::Result<()> {
    let cli = Args::parse();
    setup_logger().with_context(|| "couldn't setup logger")?;
    log_version();

    let iptables_guard = pushack_firewall::drop_rst(&cli)
        .with_context(|| "couldn't add firewall rule to drop rst")?;
    // TODO: this will not get called on panics of forwarder and ...
    // maybe launch forwarder on new process and disable panic abort and remove panics in forwarder lib
    ctrlc::set_handler(move || {
        drop(iptables_guard.clone());
        std::process::exit(1);
    })
    .unwrap();

    if cli.listen_uri.addr.is_ipv6() && cli.listen_uri.protocol == Protocol::Pushack
        || cli.remote_uri.addr.is_ipv6() && cli.remote_uri.protocol == Protocol::Pushack
    {
        bail!("pushack protocol does not support ipv6 yet");
    }

    if cli.child || !cli.reverse {
        forwarder::run(cli.listen_uri, cli.remote_uri, cli.passphrase, cli.reverse)?;
    } else {
        run_reverse(cli)?;
    }
    Ok(())
}

fn run_reverse(cli: Args) -> anyhow::Result<()> {
    ensure!(
        cli.listen_uri.protocol == Protocol::Icmp || cli.remote_uri.protocol == Protocol::Icmp,
        "you can only use reverse mode when you are using icmp protocol"
    );
    ensure!(
        cli.reverse_addr.is_none() || cli.listen_uri.protocol == Protocol::Icmp,
        "in reverse mode when you listen on icmp you need to provide reverse_addr"
    );

    let shutdown_keepalive = if let Some(reverse_addr) = cli.reverse_addr {
        health_check::initiate_client(&cli, reverse_addr)
    } else {
        health_check::initiate_server(&cli)
    }
    .with_context(|| "couldn't initiate connection")?;

    let mut args = std::env::args();
    let program = args.next().unwrap();
    let mut child_pid = Command::new(program)
        .args(args)
        .arg("--child")
        .spawn()
        .with_context(|| "couldn't spawn child")?;

    loop {
        std::thread::sleep(Duration::from_millis(300));
        if child_pid.try_wait()?.is_some() {
            break;
        }
        if shutdown_keepalive.load(Ordering::Relaxed) {
            log::info!("keepalive got shutdown");
            child_pid.kill()?;
            break;
        }
    }
    shutdown_keepalive.store(true, Ordering::Release);
    run_reverse(cli)
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
