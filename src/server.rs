use std::{net::{UdpSocket, SocketAddr}, collections::HashMap, sync::{Arc, Mutex}};
use anyhow::{Result, Context};
use log::info;
use tokio::task::yield_now;
use crate::client::{ClientData, setup_new_client};

pub async fn run_server(listen_addr: &str, redirect_addr: &str) -> Result<()> {
    let socket = UdpSocket::bind(listen_addr)
        .with_context(|| format!("Couldn't listen on '{listen_addr}'"))?;
    socket
        .set_nonblocking(true)
        .with_context(|| "Couldn't set server socket to nonblocking")?;

    info!("listen on '{listen_addr}'");
    let mut socket_map: HashMap<SocketAddr, Arc<Mutex<ClientData>>> = HashMap::new();
    let mut buffer = vec![0u8; 2048];
    loop {
        send_received_datas(&socket, &mut socket_map);

        let Ok((len, addr)) = socket.recv_from(&mut buffer) else {
            yield_now().await;
            continue;
        };

        let client_data = match socket_map.get_mut(&addr) {
            Some(data) => data,
            None => {
                info!("new client '{addr}'");
                setup_new_client(&redirect_addr, &mut socket_map, addr).await?;
                socket_map.get_mut(&addr).unwrap()
            }
        };

        let mut datas = client_data.lock().unwrap();
        datas.datas_need_to_send.push(buffer[..len].to_vec());
    }
}

#[inline]
fn send_received_datas(
    socket: &UdpSocket,
    socket_map: &mut HashMap<SocketAddr, Arc<Mutex<ClientData>>>,
) {
    for (client_socket, datas) in socket_map.iter() {
        let mut datas = datas.lock().unwrap();
        if datas.datas_received.is_empty() {
            continue;
        }

        for data in &datas.datas_received {
            let res = socket.send_to(&data, client_socket);
            if let Err(e) = res {
                info!("couldn't send back datas received from remote: {e:?}");
            }
        }
        datas.datas_received.clear();
    }
}

