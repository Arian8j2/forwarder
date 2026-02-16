use super::health_check;
use crate::Args;
use anyhow::{ensure, Context};
use forwarder::uri::Protocol;
use std::{process::Command, sync::atomic::Ordering, time::Duration};

pub fn run_reverse(cli: Args) -> anyhow::Result<()> {
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
