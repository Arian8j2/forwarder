use crate::{encryption, macros::loop_select, server::OwnnedData};
use anyhow::Result;
use log::info;
use std::net::SocketAddr;
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, Sender},
};

const MAX_CLIENT_QUEUE_SIZE: usize = 512;

pub struct Client {
    pub real_client_addr: SocketAddr,
    socket: UdpSocket,
    passphrase: Option<String>,
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
            passphrase: None,
        })
    }

    pub async fn connect(
        &mut self,
        redirect_addr: SocketAddr,
        passphrase: Option<String>,
    ) -> Result<()> {
        self.socket.connect(&redirect_addr).await?;
        self.passphrase = passphrase;
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
                //                               e                                  d
                // client -> (f1 server ---> f1 client) ------> (f2 server ---> f2 client) -> wireguard
                let data = match &self.passphrase {
                    Some(passphrase) => encryption::xor_small_chunk(data, &passphrase),
                    None => data,
                };
                self.socket.send(&data).await.ok();
            }
        }
    }

    #[inline]
    async fn handle_incomming_packets(&self, data: Vec<u8>, server_tx: Sender<OwnnedData>) {
        //                               d      network                      e
        // client <- (f1 server <--- f1 client) <------ (f2 server <--- f2 client) <- wireguard
        let data = match &self.passphrase {
            Some(passphrase) => encryption::xor_small_chunk(data, &passphrase),
            None => data,
        };

        let ownned_data = OwnnedData {
            data,
            target: self.real_client_addr,
        };

        server_tx.send(ownned_data).await.ok();
    }
}
