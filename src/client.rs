use anyhow::Result;
use log::{info, warn};
use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};
use tokio::task::yield_now;

pub struct ClientData {
    pub datas_received: Vec<Vec<u8>>,
    pub datas_need_to_send: Vec<Vec<u8>>,
}

pub async fn setup_new_client(
    redirect_addr: SocketAddr,
    socket_map: &mut HashMap<SocketAddr, Arc<Mutex<ClientData>>>,
    client_addr: SocketAddr,
) -> Result<()> {
    let client_socket = UdpSocket::bind("0.0.0.0:0")?;
    client_socket.set_nonblocking(true).unwrap();
    client_socket.connect(&redirect_addr).unwrap();

    info!(
        "created client socket '{}' for handling '{client_addr}'",
        client_socket.local_addr().unwrap()
    );
    let datas = Arc::new(Mutex::new(ClientData {
        datas_received: Vec::new(),
        datas_need_to_send: Vec::new(),
    }));
    socket_map.insert(client_addr, datas.clone());

    tokio::spawn(client_task(client_socket, datas));
    Ok(())
}

async fn client_task(client_socket: UdpSocket, datas: Arc<Mutex<ClientData>>) {
    let mut buffer = vec![0u8; 2048];
    loop {
        send_datas_need_to_send(&client_socket, &datas);
        let Ok(len) = client_socket.recv(&mut buffer) else {
            yield_now().await;
            continue;
        };

        let mut datas = datas.lock().unwrap();
        datas.datas_received.push(buffer[..len].to_vec());
    }
}

#[inline]
fn send_datas_need_to_send(client_socket: &UdpSocket, datas: &Arc<Mutex<ClientData>>) {
    let mut datas = datas.lock().unwrap();
    while let Some(data) = datas.datas_need_to_send.pop() {
        let res = client_socket.send(&data);
        if let Err(e) = res {
            warn!("failed to send packet from remote client: {e}");
        }
    }
}
