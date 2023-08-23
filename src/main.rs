mod server;

use std::{
    collections::HashMap,
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use tokio::{
    net::{UdpSocket},
    task::yield_now,
};

const LISTEN_ADDR: &str = "0.0.0.0:3939";
pub const REDIRECT_ADDR: &str = "0.0.0.0:8585";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    // let server = Server::from_addr(LISTEN_ADDR).await?;
    // server.run().await;
    start(LISTEN_ADDR, REDIRECT_ADDR).await
}

async fn start(listen_addr: &str, redirect_addr: &str) -> Result<()> {
    let socket = std::net::UdpSocket::bind(listen_addr).unwrap();
    socket.set_nonblocking(true).expect("Cannot enable non blocking socket");

    let mut socket_map: HashMap<SocketAddr, Arc<Mutex<ClientData>>> = HashMap::new();

    loop {
        for (key, val) in socket_map.iter() {
            let mut val = val.lock().unwrap();
            if !val.datas_received.is_empty() {
                for data in &val.datas_received {
                    socket.send_to(&data, key).unwrap();
                }
                val.datas_received.clear();
            }
        }

        let mut buffer = vec![0u8; 2048];
        let Ok((len, addr)) = socket.recv_from(&mut buffer) else {
            yield_now().await;
            continue;
        };
        unsafe {
            buffer.set_len(len);
        }

        let client_data = match socket_map.get_mut(&addr) {
            Some(data) => data,
            None => {
                setup_new_client(&redirect_addr, &mut socket_map, addr).await?;
                socket_map.get_mut(&addr).unwrap()
            }
        };

        let datas_received = {
            let mut datas = client_data.lock().unwrap();
            datas.datas_need_to_send.push(buffer);

            let datas_received = datas.datas_received.to_vec();
            datas.datas_received.clear();
            datas_received
        };
    }
}

struct ClientData {
    datas_received: Vec<Vec<u8>>,
    datas_need_to_send: Vec<Vec<u8>>,
}

async fn setup_new_client(
    redirect_addr: &str,
    socket_map: &mut HashMap<SocketAddr, Arc<Mutex<ClientData>>>,
    client_addr: SocketAddr,
) -> Result<()> {
    let client_socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    client_socket.set_nonblocking(true).unwrap();
    client_socket.connect(&redirect_addr).unwrap();

    let datas = Arc::new(Mutex::new(ClientData {
        datas_received: Vec::new(),
        datas_need_to_send: Vec::new(),
    }));
    socket_map.insert(client_addr, datas.clone());

    tokio::spawn(async move {
        loop {
            let datas_need_to_send = {
                let mut datas = datas.lock().unwrap();
                let res = datas.datas_need_to_send.to_vec();
                datas.datas_need_to_send.clear();
                res
            };

            for data in datas_need_to_send {
                client_socket.send(&data).unwrap();
            }

            let mut second_buffer = vec![0u8; 2048];
            let Ok(len) = client_socket.recv(&mut second_buffer) else {
                yield_now().await;
                continue;
            };

            println!("yes received");
            unsafe {
                second_buffer.set_len(len);
            }

            {
                let mut datas = datas.lock().unwrap();
                datas.datas_received.push(second_buffer);
            }
            yield_now().await;
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use ntest::timeout;
    use tokio::time::sleep;

    #[tokio::test(flavor = "multi_thread")]
    #[timeout(4000)]
    async fn test_redirect_packets() {
        let redirect_addr = "0.0.0.0:8585";
        let server_addr = "0.0.0.0:3939";
        tokio::spawn(start(server_addr, redirect_addr));

        let redirect_thread = tokio::spawn(async move {
            let server = UdpSocket::bind(redirect_addr).await?;
            let mut buf = vec![0u8; 2048];

            let len = server.recv(&mut buf).await?;
            unsafe {
                buf.set_len(len);
            }

            assert_eq!(buf, vec![1, 2, 3, 4]);
            anyhow::Ok(())
        });

        tokio::spawn(async move {
            sleep(Duration::from_millis(300)).await;
            let client = UdpSocket::bind("0.0.0.0:0").await?;
            client.connect(server_addr).await?;
            client.send(&vec![1, 2, 3, 4]).await?;
            anyhow::Ok(())
        });

        redirect_thread.await.unwrap().unwrap();
        // client_mock_thread.await.unwrap().unwrap();
    }
}
