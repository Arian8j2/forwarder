use crate::socket::{NonBlockingSocket, NonBlockingSocketTrait};
use mio::{unix::SourceFd, Interest, Token};

// also need Sync because parking_lot::RwLock needs inner to be Sync
pub trait Registry: Send + Sync {
    fn register(&self, socket: &NonBlockingSocket) -> anyhow::Result<()>;
    fn deregister(&self, socket: &NonBlockingSocket) -> anyhow::Result<()>;
}

#[derive(Debug)]
pub struct UdpRegistry(pub mio::Registry);
impl Registry for UdpRegistry {
    fn register(&self, socket: &NonBlockingSocket) -> anyhow::Result<()> {
        let NonBlockingSocket::Udp(socket) = socket else {
            unreachable!()
        };
        let local_port = socket.local_addr()?.port();
        self.0.register(
            &mut SourceFd(&socket.as_raw_fd()),
            Token(local_port.into()),
            Interest::READABLE,
        )?;
        Ok(())
    }

    fn deregister(&self, socket: &NonBlockingSocket) -> anyhow::Result<()> {
        let NonBlockingSocket::Udp(socket) = socket else {
            unreachable!()
        };
        let raw_fd = socket.as_raw_fd();
        let source = &mut SourceFd(&raw_fd);
        self.0.deregister(source)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct IcmpRegistry;
// icmp doesn't need a registry because we manage it's poll ourself
impl Registry for IcmpRegistry {
    fn register(&self, _socket: &NonBlockingSocket) -> anyhow::Result<()> {
        Ok(())
    }
    fn deregister(&self, _socket: &NonBlockingSocket) -> anyhow::Result<()> {
        Ok(())
    }
}
