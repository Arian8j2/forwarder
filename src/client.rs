use crate::{macros::loop_select, server::OwnnedData};
use anyhow::Result;
use log::{info, warn};
use std::net::SocketAddr;
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, Sender},
};

const MAX_CLIENT_QUEUE_SIZE: usize = 512;

pub struct Client {
    pub real_client_addr: SocketAddr,
    socket: UdpSocket,
}

impl Client {
    pub async fn new(real_client_addr: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

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

    pub async fn connect(&self, redirect_addr: SocketAddr) -> Result<()> {
        self.socket.connect(&redirect_addr).await?;
        Ok(())
    }

    pub fn spawn_task(self, server_tx: Sender<OwnnedData>) -> Sender<Vec<u8>> {
        let (client_tx, mut client_rx) = mpsc::channel::<Vec<u8>>(MAX_CLIENT_QUEUE_SIZE);
        let mut buffer = vec![0u8; 2048];

        tokio::spawn(async move {
            loop_select! {
                datas_need_to_send = client_rx.recv() => self.handle_datas_need_to_send(datas_need_to_send).await,
                Ok(len) = self.socket.recv(&mut buffer) => {
                    let data = buffer[..len].to_vec();
                    self.handle_incomming_packets(data, server_tx.clone()).await;
                }
            };
        });

        client_tx.clone()
    }

    #[inline]
    async fn handle_datas_need_to_send(&self, datas_need_to_send: Option<Vec<u8>>) {
        match datas_need_to_send {
            None => panic!("client channel has been closed FIXME"),
            Some(data) => {
                self.socket.send(&data).await.ok();
            }
        }
    }

    #[inline]
    async fn handle_incomming_packets(&self, data: Vec<u8>, server_tx: Sender<OwnnedData>) {
        let ownned_data = OwnnedData {
            data,
            target: self.real_client_addr,
        };

        if let Err(e) = server_tx.send(ownned_data).await {
            warn!("send to channel failed: {e}");
        }
    }
}
