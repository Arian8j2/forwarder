use crate::{client::Client, macros::loop_select};
use anyhow::{Context, Result};
use log::{info, warn};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{self, Receiver, Sender};

const MAX_SERVER_QUEUE_SIZE: usize = 1024;
const CLIENTS_BASE_CAPACITY: usize = 100;

pub struct OwnnedData {
    pub data: Vec<u8>,
    pub target: SocketAddr,
}

struct ReceiverClient {
    addr: SocketAddr,
    tx: Sender<Vec<u8>>,
}

pub struct Server {
    socket: UdpSocket,
    send_to_real_client_tx: Sender<OwnnedData>,
    send_to_real_client_rx: Receiver<OwnnedData>,
    clients: Vec<ReceiverClient>,
}

impl Server {
    pub async fn bind(listen_addr: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind(listen_addr)
            .await
            .with_context(|| format!("Couldn't listen on '{listen_addr}'"))?;
        info!("listen on '{listen_addr}'");

        let (tx, rx) = mpsc::channel::<OwnnedData>(MAX_SERVER_QUEUE_SIZE);
        let clients: Vec<ReceiverClient> = Vec::with_capacity(CLIENTS_BASE_CAPACITY);
        Ok(Self {
            socket,
            send_to_real_client_tx: tx,
            send_to_real_client_rx: rx,
            clients,
        })
    }

    pub async fn run(mut self, redirect_addr: SocketAddr) {
        let mut buffer = vec![0u8; 2048];

        loop_select! {
            data_need_to_send = self.send_to_real_client_rx.recv() => {
                match data_need_to_send {
                    None => {
                        warn!("server mpsc channel got disconnected");
                        break;
                    },
                    Some(ownned_data) => self.send_data_to(&ownned_data.data, ownned_data.target).await
                }
            },
            Ok((len, from_addr)) = self.socket.recv_from(&mut buffer) => {
                let data = buffer[..len].to_vec();
                self.handle_incomming_packet(from_addr, data, redirect_addr)
                    .await;
            }
        }
    }

    #[inline]
    async fn send_data_to(&self, data: &Vec<u8>, target: SocketAddr) {
        let res = self.socket.send_to(&data, target).await;
        if let Err(e) = res {
            warn!("couldn't send back datas received from remote: {e}");
        }
    }

    #[inline]
    async fn handle_incomming_packet(
        &mut self,
        from_addr: SocketAddr,
        data: Vec<u8>,
        redirect_addr: SocketAddr,
    ) {
        let client_tx = match self.clients.iter().find(|c| c.addr == from_addr) {
            Some(client) => &client.tx,
            None => {
                info!("new client '{from_addr}'");
                let Ok(new_client) = prepare_new_client(from_addr, redirect_addr).await else {
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
}

async fn prepare_new_client(
    real_client_addr: SocketAddr,
    redirect_addr: SocketAddr,
) -> Result<Client> {
    let new_client = Client::new(real_client_addr).await?;
    new_client.connect(redirect_addr).await?;
    Ok(new_client)
}
