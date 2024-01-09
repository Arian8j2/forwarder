use crate::socket::SocketUri;
use clap::Parser;

/// Simple program to forward udp packets
#[derive(Parser)]
#[command(about)]
pub struct Args {
    #[arg(short, long)]
    pub listen_addr: SocketUri,

    #[arg(short, long)]
    pub remote_addr: SocketUri,

    #[arg(
        short,
        long,
        help = "The packets will get encrypted/decrypted by this passphrase"
    )]
    pub passphrase: Option<String>,
}
