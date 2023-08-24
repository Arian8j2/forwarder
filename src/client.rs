use crate::server::OwnnedData;
use anyhow::Result;
use log::{info, warn};
use std::net::{SocketAddr, UdpSocket};
use tokio::sync::mpsc::{self, error::TryRecvError, Sender};

pub struct Client {
    pub real_client_addr: SocketAddr,
    socket: UdpSocket,
}

impl Client {
    pub fn new(real_client_addr: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;

        info!(
            "created client socket '{}' for handling '{}'",
            socket.local_addr().unwrap(),
            real_client_addr
        );
        Ok(Client {
            real_client_addr,
            socket,
        })
    }

    pub fn connect(&self, redirect_addr: SocketAddr) -> Result<()> {
        self.socket.connect(&redirect_addr)?;
        Ok(())
    }

    pub fn spawn_task(self, server_tx: Sender<OwnnedData>) -> Sender<Vec<u8>> {
        let (client_tx, mut client_rx) = mpsc::channel::<Vec<u8>>(512);
        let mut buffer = vec![0u8; 2048];

        tokio::spawn(async move {
            loop {
                match client_rx.try_recv() {
                    Ok(data) => {
                        if let Err(e) = self.socket.send(&data) {
                            warn!("error when sending this: {e}");
                        }
                    }
                    Err(TryRecvError::Empty) => {
                        let Ok(len) = self.socket.recv(&mut buffer) else {
                            continue;
                        };

                        let data = OwnnedData {
                            data: buffer[..len].to_vec(),
                            target: self.real_client_addr,
                        };
                        if let Err(e) = server_tx.send(data).await {
                            warn!("send to channel failed: {e}");
                        }
                    }
                    Err(TryRecvError::Disconnected) => {
                        info!("client mspc channel disconnect, FIXME");
                    }
                }
            }
        });

        client_tx.clone()
    }
}
