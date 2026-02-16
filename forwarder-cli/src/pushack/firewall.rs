use super::Args;
use anyhow::{bail, Context};
use forwarder::uri::Protocol;
use std::process::Command;

#[derive(Default, Clone)]
pub struct IptablesGuard {
    filters: Vec<Filter>,
}

impl Drop for IptablesGuard {
    fn drop(&mut self) {
        for filter in self.filters.drain(..) {
            run_iptables_rst_filter(Action::Remove, filter)
                .with_context(|| "couldn't remove iptables filter")
                .unwrap();
        }
    }
}

pub fn drop_rst(cli: &Args) -> anyhow::Result<IptablesGuard> {
    let mut guard = IptablesGuard::default();
    if cli.remote_uri.protocol == Protocol::Pushack {
        let filter = Filter::DestPort(cli.remote_uri.addr.port());
        run_iptables_rst_filter(Action::Add, filter)
            .with_context(|| "couldn't add rst filter for remote")?;
        guard.filters.push(filter);
    }
    if cli.listen_uri.protocol == Protocol::Pushack {
        let filter = Filter::SourcePort(cli.listen_uri.addr.port());
        run_iptables_rst_filter(Action::Add, filter)
            .with_context(|| "couldn't add rst filter for listen")?;
        guard.filters.push(filter);
    }
    Ok(guard)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Filter {
    SourcePort(u16),
    DestPort(u16),
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Action {
    Add,
    Remove,
}

fn run_iptables_rst_filter(action: Action, filter: Filter) -> anyhow::Result<()> {
    let (filter, value) = match filter {
        Filter::DestPort(port) => ("--dport", port.to_string()),
        Filter::SourcePort(port) => ("--sport", port.to_string()),
    };
    run_command(
        "iptables",
        &[
            "-t",
            "mangle",
            if action == Action::Add { "-I" } else { "-D" },
            "POSTROUTING",
            "-p",
            "tcp",
            filter,
            &value,
            "--tcp-flags",
            "RST",
            "RST",
            "-j",
            "DROP",
        ],
    )
    .with_context(|| "iptables command failed")?;
    Ok(())
}

fn run_command(program: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("couldn't spawn '{program}' program"))?;
    let stdout = String::from_utf8(output.stdout)?;
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr)?;
        bail!("{stdout}\n{stderr}")
    }
    Ok(stdout)
}
