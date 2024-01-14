use crate::{
    encryption,
    macros::loop_select,
    server::{OwnnedData, MAX_PACKET_SIZE},
    socket::{Socket, SocketProtocol},
};
use anyhow::Result;
use std::net::SocketAddrV4;
use tokio::sync::mpsc::{self, Sender};

const MAX_CLIENT_TO_SERVER_CHANNEL_QUEUE_SIZE: usize = 512;

pub struct Client {
    real_client_addr: SocketAddrV4,
    socket: Box<dyn Socket>,
    passphrase: Option<String>,
}

impl Client {
    pub async fn new(
        socket_protocol: SocketProtocol,
        real_client_addr: SocketAddrV4,
    ) -> Result<Self> {
        let addr: SocketAddrV4 = "0.0.0.0:0".parse()?;
        let socket = socket_protocol.bind(&addr).await?;

        // TODO: add some log message similar to this
        // info!(
        //     "created client socket '{}' for handling '{}'",
        //     socket.local_addr().unwrap(),
        //     real_client_addr
        // );

        Ok(Client {
            real_client_addr,
            socket,
            passphrase: None,
        })
    }

    pub async fn connect(
        &mut self,
        redirect_addr: SocketAddrV4,
        passphrase: Option<String>,
    ) -> Result<()> {
        self.socket.connect(&redirect_addr).await?;
        self.passphrase = passphrase;
        Ok(())
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
