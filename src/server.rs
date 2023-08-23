// use std::{task::Poll, net::SocketAddr, future::Future, pin::Pin, time::Duration};
// use anyhow::Result;
// use tower::Service;
//
// pub struct Server {
//     clients: Vec<SocketAddr>
// }
//
// impl Server {
//     pub fn new() -> Self {
//         Server {
//             clients: Vec::new()
//         }
//     }
// }
//
// pub struct Request {
//     pub address: SocketAddr,
//     pub message: Vec<u8>
// }
//
// impl Service<Request> for Server {
//     type Error = anyhow::Error;
//     type Response = Vec<u8>;
//     type Future = Pin<Box<dyn Future<Output = Result<Self::Response>>>>;
//
//     fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<()>> {
//         Poll::Ready(Ok(()))
//     }
//
//     fn call(&mut self, req: Request) -> Self::Future {
//         self.clients.push(req.address);
//         println!("msg: {}", String::from_utf8(req.message).unwrap());
//         Box::pin(async move {
//             Ok(vec![1, 2, 4, 5])
//         })
//     }
// }
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
//
// // use std::{net::SocketAddr, time::Duration, sync::{Arc, Mutex}};
// //
// // use anyhow::{Result, bail};
// // use tokio::{net::{UdpSocket}, time::sleep};
// //
// // use crate::REDIRECT_ADDR;
// //
// // const MAX_BUFFER_SIZE: usize = 2048;
// //
// // pub struct Server {
// //     socket: UdpSocket,
// //     clients: Arc<Mutex<Vec<Client>>>,
// // }
// //
// // impl Server {
// //     pub async fn from_addr(addr: &str) -> Result<Server> {
// //         let socket = UdpSocket::bind(addr).await?;
// //         Ok(Server { socket, clients: Vec::new() })
// //     }
// //
// //     pub async fn run(mut self) {
// //
// //         loop {
// //             let (message, addr) = match self.receive_message().await {
// //                 Ok(m) => m,
// //                 Err(err) => {
// //                     println!("cycle error: {err}");
// //                     continue;
// //                 }
// //             };
// //
// //             let mut found = false;
// //             for client in self.clients.iter() {
// //                 let mut client = client.lock().await;
// //                 if client.addr != addr {
// //                     continue;
// //                 }
// //
// //                 found = true;
// //                 client.pending_datas.push(message);
// //                 break;
// //             }
// //
// //             if !found {
// //
// //             }
// //
// //             // println!("msg: '{}'", String::from_utf8(message).unwrap());
// //         }
// //     }
// //
// //     pub async fn receive_message(&mut self) -> Result<(Vec<u8>, SocketAddr)> {
// //         let mut buffer = vec![0u8; MAX_BUFFER_SIZE];
// //         let (len, addr) = self.socket.recv_from(&mut buffer).await?;
// //         unsafe {
// //             buffer.set_len(len);
// //         }
// //
// //         if buffer.len() == MAX_BUFFER_SIZE {
// //             bail!("Maybe buffer size was not enough, Make sure to set MTU lower than {MAX_BUFFER_SIZE}");
// //         }
// //
// //         Ok((buffer, addr))
// //     }
// //
// //     async fn client_thread(addr: SocketAddr) -> Result<()> {
// //         let client = Client::from_addr(addr).await?;
// //         client.socket.connect(REDIRECT_ADDR).await?;
// //
// //         loop {
// //             let mut datas = client.pending_datas.lock().await;
// //             if datas.is_empty() {
// //                 println!("empty");
// //                 drop(datas);
// //                 sleep(Duration::from_millis(10)).await;
// //                 continue;
// //             }
// //
// //             for data in datas.iter() {
// //                 let Err(err) = client.socket.send(data).await else {
// //                     continue;
// //                 };
// //
// //                 println!("error while sending client data: {err}");
// //             }
// //             datas.clear();
// //         }
// //     }
// // }
// //
// // struct Client {
// //     addr: SocketAddr,
// //     socket: UdpSocket,
// //
// //     // vector that contains multiple packets
// //     pending_datas: Vec<Vec<u8>>,
// // }
// //
// // impl Client {
// //     async fn from_addr(addr: SocketAddr) -> Result<Self> {
// //         let socket = UdpSocket::bind("0.0.0.0:0").await?;
// //         Ok(Client {
// //             addr,
// //             socket,
// //             pending_datas: Vec::new()
// //         })
// //     }
// // }
