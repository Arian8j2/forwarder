use clap::Parser;
use std::net::SocketAddrV4;

/// Simple program to forward udp packets
#[derive(Parser)]
#[command(about)]
pub struct Args {
    #[arg(short, long)]
    pub listen_addr: SocketAddrV4,

    #[arg(short, long)]
    pub remote_addr: SocketAddrV4,
}
