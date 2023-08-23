use crate::client::{setup_new_client, Client};
use anyhow::{Context, Result};
use log::{info, warn};
use std::net::{SocketAddr, UdpSocket};
use tokio::task::yield_now;

pub async fn run_server(listen_addr: SocketAddr, redirect_addr: SocketAddr) -> Result<()> {
    let socket = UdpSocket::bind(listen_addr)
        .with_context(|| format!("Couldn't listen on '{listen_addr}'"))?;
    socket
        .set_nonblocking(true)
        .with_context(|| "Couldn't set server socket to nonblocking")?;

    info!("listen on '{listen_addr}'");
    let mut clients: Vec<Client> = Vec::with_capacity(100);
    let mut buffer = vec![0u8; 2048];
    loop {
        send_received_datas(&socket, &mut clients);
        let Ok((len, addr)) = socket.recv_from(&mut buffer) else {
            yield_now().await;
            continue;
        };

        let client = match clients.iter().find(|c| c.addr == addr) {
            Some(client) => &client,
            None => {
                info!("new client '{addr}'");
                let new_client = Client::new(addr);
                setup_new_client(&new_client, redirect_addr).await?;
                clients.push(new_client);
                clients.last().unwrap()
            }
        };

        let mut datas = client.datas.lock().unwrap();
        datas.need_to_send.push(buffer[..len].to_vec());
    }
}

#[inline]
fn send_received_datas(socket: &UdpSocket, clients: &mut Vec<Client>) {
    for client in clients.iter() {
        let mut datas = client.datas.lock().unwrap();
        if datas.received.is_empty() {
            continue;
        }

        while let Some(data) = datas.received.pop() {
            let res = socket.send_to(&data, client.addr);
            if let Err(e) = res {
                warn!("couldn't send back datas received from remote: {e}");
            }
        }
    }
}
