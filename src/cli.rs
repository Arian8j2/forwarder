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

    #[arg(long, default_value_t = 8)]
    pub icmp_type: u8,

    #[arg(long, default_value_t = 0)]
    pub icmp_code: u8,

    #[arg(
        long,
        help = "don't calculate icmp packet checksum when sending icmp packet",
        long_help = "by default we calculate checksum of each icmp packet and add it to the packet before sending it,\nnot calculating it will speed up sending packet and reduce cpu usage but packet may get dropped by some firewalls"
    )]
    pub icmp_ignore_checksum: bool,

    #[arg(
        short,
        long,
        help = "The packets will get encrypted/decrypted by this passphrase"
    )]
    pub passphrase: Option<String>,
}
