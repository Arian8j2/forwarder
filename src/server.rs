use crate::socket::{Socket, SocketUri};
use crate::{client::Client, macros::loop_select};
use anyhow::{Context, Result};
use log::{info, warn};
use std::net::SocketAddr;
use tokio::sync::mpsc::{self, Receiver, Sender};

const MAX_SERVER_CHANNEL_QUEUE_SIZE: usize = 1024;
const CLIENTS_BASE_CAPACITY: usize = 100;
pub const MAX_PACKET_SIZE: usize = 2048;

pub enum ClientToServerMsg {
    DataFromRealServer { data: Vec<u8>, target: SocketAddr },
    ClientCleanup(SocketAddr),
}

struct ReceiverClient {
    addr: SocketAddr,
    tx: Sender<Vec<u8>>,
}

pub struct Server {
    socket: Box<dyn Socket>,
    // each `Client` gets a clone of this so they can send data to server
    send_to_real_client_tx: Sender<ClientToServerMsg>,
    // receive data that needs to sent back to real client
    send_to_real_client_rx: Receiver<ClientToServerMsg>,
    // using vector instead of hashmap because there is a few clients
    // maybe around 50 or lower so finding in vector is faster
    clients: Vec<ReceiverClient>,
    passphrase: Option<String>,
}

impl Server {
    pub async fn new(uri: SocketUri) -> Result<Self> {
        let listen_addr = &uri.addr;
        let socket = uri
            .protocol
            .bind(&uri.addr)
            .await
            .with_context(|| format!("Couldn't listen on '{listen_addr}'"))?;
        info!("listen on '{listen_addr}'");

        let (tx, rx) = mpsc::channel::<ClientToServerMsg>(MAX_SERVER_CHANNEL_QUEUE_SIZE);
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

    pub async fn run(mut self, redirect_uri: SocketUri) {
        let mut buffer = vec![0u8; MAX_PACKET_SIZE];

        loop_select! {
            // receive data from `Client` and send them back to real client
            message = self.send_to_real_client_rx.recv() => {
                let message = message.expect("server-client mpsc channel closed");
                match message {
                    ClientToServerMsg::DataFromRealServer { data, target } => {
                        self.send_data_to(&data, target).await
                    },
                    ClientToServerMsg::ClientCleanup(addr) => {
                        let index = self.clients.iter().position(|client| client.addr == addr).unwrap();
                        self.clients.remove(index);
                        log::info!("cleaned client that handled '{addr}'");
                    }
                }
            },

            // receive data from real client and transfer it to `Client`
            Ok((len, from_addr)) = self.socket.recv_from(&mut buffer) => {
                let data = buffer[..len].to_vec();
                self.handle_incomming_packet(from_addr, data, &redirect_uri)
                    .await;
            },
        }
    }

    #[inline]
    async fn send_data_to(&self, data: &[u8], target: SocketAddr) {
        let res = self.socket.send_to(data, &target).await;
        if let Err(e) = res {
            warn!("couldn't send back datas received from remote: {e}");
        }
    }

    /// transfers packets to `Client` via channel
    #[inline]
    async fn handle_incomming_packet(
        &mut self,
        from_addr: SocketAddr,
        data: Vec<u8>,
        redirect_uri: &SocketUri,
    ) {
        let client_tx = match self.clients.binary_search_by_key(&from_addr, |c| c.addr) {
            Ok(index) => &self.clients[index].tx,
            Err(appropriate_index) => {
                info!("new client '{from_addr}'");
                match self
                    .add_new_client(appropriate_index, from_addr, redirect_uri)
                    .await
                {
                    Ok(new_client) => &new_client.tx,
                    Err(err) => {
                        warn!("cannot prepare new client '{from_addr}', {err:?}");
                        return;
                    }
                }
            }
        };

        let res = client_tx.send(data).await;
        if let Err(e) = res {
            warn!("cannot send datas from server to client via channel: {e}");
        }
    }

    async fn add_new_client(
        &mut self,
        index: usize,
        real_client_addr: SocketAddr,
        redirect_uri: &SocketUri,
    ) -> Result<&ReceiverClient> {
        let client_tx = Client::connect(
            real_client_addr.to_owned(),
            redirect_uri.to_owned(),
            self.passphrase.clone(),
        )
        .await?
        .spawn_task(self.send_to_real_client_tx.clone());

        let receiver_client = ReceiverClient {
            addr: real_client_addr,
            tx: client_tx,
        };
        self.clients.insert(index, receiver_client);
        Ok(&self.clients[index])
    }
}
