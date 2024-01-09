use crate::socket::{Socket, SocketVariant};
use crate::{client::Client, macros::loop_select};
use anyhow::{Context, Result};
use log::{info, warn};
use std::net::SocketAddrV4;
use tokio::sync::mpsc::{self, Receiver, Sender};

const MAX_SERVER_CHANNEL_QUEUE_SIZE: usize = 1024;
const CLIENTS_BASE_CAPACITY: usize = 100;

pub struct OwnnedData {
    pub data: Vec<u8>,
    pub target: SocketAddrV4,
}

struct ReceiverClient {
    addr: SocketAddrV4,
    tx: Sender<Vec<u8>>,
}

pub struct Server {
    socket: Box<dyn Socket>,
    // each `Client` gets a clone of this so they can send data to server
    send_to_real_client_tx: Sender<OwnnedData>,
    // receive data that needs to sent back to real client
    send_to_real_client_rx: Receiver<OwnnedData>,
    // using vector instead of hashmap because there is a few clients
    // maybe around 50 or lower so finding in vector is faster
    clients: Vec<ReceiverClient>,
    passphrase: Option<String>,
}

impl Server {
    pub async fn new(
        listen_socket_variant: SocketVariant,
        listen_addr: &SocketAddrV4,
    ) -> Result<Self> {
        let socket = listen_socket_variant
            .bind(&listen_addr)
            .await
            .with_context(|| format!("Couldn't listen on '{listen_addr}'"))?;
        info!("listen on '{listen_addr}'");

        let (tx, rx) = mpsc::channel::<OwnnedData>(MAX_SERVER_CHANNEL_QUEUE_SIZE);
        let clients: Vec<ReceiverClient> = Vec::with_capacity(CLIENTS_BASE_CAPACITY);
        Ok(Self {
            socket,
            send_to_real_client_tx: tx,
            send_to_real_client_rx: rx,
            clients,
            passphrase: None,
        })
    }

    pub fn set_passphrase(&mut self, passphrase: &str) {
        self.passphrase = Some(passphrase.to_string());
    }

    pub async fn run(
        mut self,
        redirect_addr: SocketAddrV4,
        redirect_socket_variant: SocketVariant,
    ) {
        let mut buffer = vec![0u8; 2048];

        loop_select! {
            // receive data from `Client` and send them back to real client
            data_need_to_send = self.send_to_real_client_rx.recv() => {
                let Some(ownned_data) = data_need_to_send else {
                    panic!("server mpsc channel got disconnected");
                };
                self.send_data_to(&ownned_data.data, ownned_data.target).await
            },
            // receive data from real client and transfer it to `Client`
            Ok((len, from_addr)) = self.socket.recv_from(&mut buffer) => {
                let data = buffer[..len].to_vec();
                self.handle_incomming_packet(from_addr, data, &redirect_socket_variant, redirect_addr)
                    .await;
            }
        }
    }

    #[inline]
    async fn send_data_to(&self, data: &Vec<u8>, target: SocketAddrV4) {
        let res = self.socket.send_to(&data, &target).await;
        if let Err(e) = res {
            warn!("couldn't send back datas received from remote: {e}");
        }
    }

    /// transfers packets to `Client` via channel
    #[inline]
    async fn handle_incomming_packet(
        &mut self,
        from_addr: SocketAddrV4,
        data: Vec<u8>,
        redirect_socket_variant: &SocketVariant,
        redirect_addr: SocketAddrV4,
    ) {
        let client_tx = match self.clients.iter().find(|c| c.addr == from_addr) {
            Some(client) => &client.tx,
            None => {
                info!("new client '{from_addr}'");
                let Ok(new_client) = self
                    .prepare_new_client(
                        from_addr,
                        redirect_socket_variant.to_owned(),
                        redirect_addr,
                        &self.passphrase,
                    )
                    .await
                else {
                    warn!("cannot prepare new client '{from_addr}'");
                    return;
                };
                let client_tx = new_client.spawn_task(self.send_to_real_client_tx.clone());

                self.clients.push(ReceiverClient {
                    addr: from_addr,
                    tx: client_tx,
                });
                &self.clients.last().unwrap().tx
            }
        };

        let res = client_tx.send(data).await;
        if let Err(e) = res {
            warn!("cannot send datas from server to client via channel: {e}");
        }
    }

    async fn prepare_new_client(
        &self,
        real_client_addr: SocketAddrV4,
        redirect_socket_variant: SocketVariant,
        redirect_addr: SocketAddrV4,
        passphrase: &Option<String>,
    ) -> Result<Client> {
        let mut new_client = Client::new(redirect_socket_variant, real_client_addr).await?;
        new_client
            .connect(redirect_addr, passphrase.clone())
            .await?;
        Ok(new_client)
    }
}
