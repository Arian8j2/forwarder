use anyhow::Result;
use log::{info, warn};
use std::{
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};
use tokio::task::yield_now;

pub struct Client {
    pub addr: SocketAddr,
    pub datas: Arc<Mutex<Data>>,
}

pub struct Data {
    pub received: Vec<Vec<u8>>,
    pub need_to_send: Vec<Vec<u8>>,
}

impl Client {
    pub fn new(addr: SocketAddr) -> Self {
        let datas = Arc::new(Mutex::new(Data {
            received: Vec::new(),
            need_to_send: Vec::new(),
        }));
        Client { addr, datas }
    }
}

pub async fn setup_new_client(new_client: &Client, redirect_addr: SocketAddr) -> Result<()> {
    let client_socket = UdpSocket::bind("0.0.0.0:0")?;
    client_socket.set_nonblocking(true)?;
    client_socket.connect(&redirect_addr)?;

    info!(
        "created client socket '{}' for handling '{}'",
        client_socket.local_addr().unwrap(),
        new_client.addr
    );

    tokio::spawn(client_task(client_socket, new_client.datas.clone()));
    Ok(())
}

async fn client_task(client_socket: UdpSocket, datas: Arc<Mutex<Data>>) {
    let mut buffer = vec![0u8; 2048];
    loop {
        send_datas_need_to_send(&client_socket, &datas);
        let Ok(len) = client_socket.recv(&mut buffer) else {
            yield_now().await;
            continue;
        };

        let mut datas = datas.lock().unwrap();
        datas.received.push(buffer[..len].to_vec());
    }
}

#[inline]
fn send_datas_need_to_send(client_socket: &UdpSocket, datas: &Arc<Mutex<Data>>) {
    let mut datas = datas.lock().unwrap();
    while let Some(data) = datas.need_to_send.pop() {
        let res = client_socket.send(&data);
        if let Err(e) = res {
            warn!("failed to send packet from remote client: {e}");
        }
    }
}
