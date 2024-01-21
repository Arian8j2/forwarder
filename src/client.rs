use crate::{
    encryption,
    macros::loop_select,
    server::{OwnnedData, MAX_PACKET_SIZE},
    socket::{Socket, SocketUri},
};
use anyhow::{Context, Result};
use log::info;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use tokio::sync::mpsc::{self, Sender};

const MAX_CLIENT_TO_SERVER_CHANNEL_QUEUE_SIZE: usize = 512;

pub struct Client {
    real_client_addr: SocketAddr,
    socket: Box<dyn Socket>,
    passphrase: Option<String>,
}

impl Client {
    pub async fn connect(
        real_client_addr: SocketAddr,
        redirect_uri: SocketUri,
        passphrase: Option<String>,
    ) -> Result<Self> {
        let addr: SocketAddr = match redirect_uri.addr {
            SocketAddr::V4(_) => SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0).into(),
            SocketAddr::V6(_) => {
                SocketAddrV6::new(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0), 0, 0, 0).into()
            }
        };

        let mut socket = redirect_uri
            .protocol
            .bind(&addr)
            .await
            .with_context(|| "Binding address to Client")?;
        socket
            .connect(&redirect_uri.addr)
            .await
            .with_context(|| "Connecting to redirect address")?;

        info!(
            "created client socket '{}/{}' for handling '{}'",
            socket.local_addr().unwrap(),
            redirect_uri.protocol,
            real_client_addr
        );

        Ok(Client {
            real_client_addr,
            socket,
            passphrase,
        })
    }

    pub fn spawn_task(mut self, server_tx: Sender<OwnnedData>) -> Sender<Vec<u8>> {
        let (client_tx, mut client_rx) =
            mpsc::channel::<Vec<u8>>(MAX_CLIENT_TO_SERVER_CHANNEL_QUEUE_SIZE);
        let mut buffer = vec![0u8; MAX_PACKET_SIZE];

        tokio::spawn(async move {
            loop_select! {
                // receive data from `Server` and send them to real server
                datas_need_to_send = client_rx.recv() => self.handle_datas_need_to_send(datas_need_to_send).await,
                // receive data from real Server and transfer them back to `Server` to handle it
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
        let Some(data) = datas_need_to_send else {
            panic!("client channel has been closed FIXME")
        };
        //                               e                                  d
        // client -> (f1 server ---> f1 client) ------> (f2 server ---> f2 client) -> wireguard
        let data = match &self.passphrase {
            Some(passphrase) => encryption::xor_small_chunk(data, passphrase),
            None => data,
        };
        self.socket.send(&data).await.ok();
    }

    #[inline]
    async fn handle_incomming_packets(&self, data: Vec<u8>, server_tx: Sender<OwnnedData>) {
        //                               d      network                      e
        // client <- (f1 server <--- f1 client) <------ (f2 server <--- f2 client) <- wireguard
        let data = match &self.passphrase {
            Some(passphrase) => encryption::xor_small_chunk(data, passphrase),
            None => data,
        };

        let ownned_data = OwnnedData {
            data,
            target: self.real_client_addr,
        };

        server_tx.send(ownned_data).await.ok();
    }
}
