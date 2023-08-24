use crate::client::Client;
use anyhow::{Context, Result};
use log::{error, info, warn};
use std::net::{SocketAddr, UdpSocket};
use tokio::sync::mpsc::{self, error::TryRecvError, Receiver, Sender};

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
    pub fn bind(listen_addr: SocketAddr) -> Result<Self> {
        let socket = setup_socket(listen_addr)?;
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

        loop {
            match self.send_to_real_client_rx.try_recv() {
                Ok(d) => self.send_data_to(&d.data, d.target),
                Err(TryRecvError::Empty) => {
                    let Ok((len, from_addr)) = self.socket.recv_from(&mut buffer) else {
                        continue;
                    };

                    let data = buffer[..len].to_vec();
                    self.handle_incomming_packet(from_addr, data, redirect_addr)
                        .await;
                }
                Err(TryRecvError::Disconnected) => {
                    error!("server mspc channel died");
                    break;
                }
            }
        }
    }

    #[inline]
    fn send_data_to(&self, data: &Vec<u8>, target: SocketAddr) {
        let res = self.socket.send_to(&data, target);
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
                let Ok(new_client) = prepare_new_client(from_addr, redirect_addr) else {
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

fn setup_socket(listen_addr: SocketAddr) -> Result<UdpSocket> {
    let socket = UdpSocket::bind(listen_addr)
        .with_context(|| format!("Couldn't listen on '{listen_addr}'"))?;
    socket
        .set_nonblocking(true)
        .with_context(|| "Couldn't set server socket to nonblocking")?;
    Ok(socket)
}

fn prepare_new_client(real_client_addr: SocketAddr, redirect_addr: SocketAddr) -> Result<Client> {
    let new_client = Client::new(real_client_addr)?;
    new_client.connect(redirect_addr)?;
    Ok(new_client)
}
