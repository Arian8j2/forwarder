use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{io::Result, mem::MaybeUninit, os::fd::AsRawFd};
use tokio::io::{unix::AsyncFd, Interest};

pub struct AsyncRawSocket {
    inner: AsyncFd<Socket>,
}

impl AsyncRawSocket {
    pub fn new(domain: Domain, protocol: Protocol) -> Result<Self> {
        let socket = Socket::new(domain, Type::RAW, Some(protocol))?;
        socket.set_nonblocking(true)?;
        Ok(AsyncRawSocket {
            inner: AsyncFd::new(socket)?,
        })
    }

    pub fn bind(&self, address: &SockAddr) -> Result<()> {
        self.inner.get_ref().bind(address)
    }

    pub async fn recv(&self, buffer: &mut [u8]) -> Result<usize> {
        self.inner
            .async_io(Interest::READABLE, |inner| {
                let buffer_maybe_uninit =
                    unsafe { &mut *(buffer as *mut [u8] as *mut [MaybeUninit<u8>]) };
                inner.recv(buffer_maybe_uninit)
            })
            .await
    }

    pub async fn send_to(&self, buffer: &[u8], to: &SockAddr) -> Result<usize> {
        self.inner
            .async_io(Interest::WRITABLE, |inner| inner.send_to(buffer, to))
            .await
    }
}

impl AsRawFd for AsyncRawSocket {
    fn as_raw_fd(&self) -> std::os::unix::prelude::RawFd {
        self.inner.as_raw_fd()
    }
}
