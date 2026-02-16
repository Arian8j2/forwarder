use crate::Args;
use anyhow::{bail, Context};
use forwarder::uri::Protocol;

mod firewall;

pub fn prepare_firewall(cli: &Args) -> anyhow::Result<()> {
    if cli.listen_uri.addr.is_ipv6() && cli.listen_uri.protocol == Protocol::Pushack
        || cli.remote_uri.addr.is_ipv6() && cli.remote_uri.protocol == Protocol::Pushack
    {
        bail!("pushack protocol does not support ipv6 yet");
    }

    let iptables_guard =
        firewall::drop_rst(cli).with_context(|| "couldn't add firewall rule to drop rst")?;

    // TODO: this will not get called on panics of forwarder and ...
    // maybe launch forwarder on new process and disable panic abort and remove panics in forwarder lib
    ctrlc::set_handler(move || {
        drop(iptables_guard.clone());
        std::process::exit(1);
    })
    .unwrap();

    Ok(())
}
