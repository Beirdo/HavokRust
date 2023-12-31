extern crate tokio;

use crate::settings::Settings;
use crate::connection::Connection;
use crate::logging::*;
use crate::ControlSignal;
use std::net::SocketAddr;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpSocket, TcpListener, TcpStream};
use tokio::io;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::sync::{broadcast, mpsc, RwLock, Barrier};
use std::sync::Arc;
use tokio::task;
use std::collections::HashMap;
use tokio::task::JoinHandle;
use bytes::BytesMut;

#[derive(Debug, Clone)]
pub struct NetworkMessage {
    pub dest: SocketAddr,
    pub data: Vec<u8>,
}


#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Server {
    initialized: bool,
    bind_ip: String,
    port: u16,
    wizlocked: bool,
    wizlock_reason: String,
    settings: Option<Settings>,
    pub connections: HashMap<SocketAddr, Connection>,
    pub wr_streams: HashMap<SocketAddr, Arc<RwLock<OwnedWriteHalf>>>,
    pub rd_streams: HashMap<SocketAddr, Arc<RwLock<OwnedReadHalf>>>,
    pub rd_handles: HashMap<SocketAddr, Arc<RwLock<JoinHandle<()>>>>,
}

use lazy_static::lazy_static;
lazy_static! {
    static ref SERVER: Arc<RwLock<Server>> = Arc::new(RwLock::new(Server {
        initialized: false,
        bind_ip: "".to_string(),
        port: 0,
        wizlocked: false,
        wizlock_reason: "".to_string(),
        settings: None,
        connections: HashMap::new(),
        wr_streams: HashMap::new(),
        rd_streams: HashMap::new(),
        rd_handles: HashMap::new(),
    }));
}


impl Server {
    pub async fn get(settings: Option<Settings>) -> Arc<RwLock<Server>> {
        let initialized = {
            SERVER.read().await.initialized
        };

        if !initialized {
            SERVER.write().await.initialize(settings);
        }
        SERVER.clone()
    }

    pub fn initialize(&mut self, settings: Option<Settings>) {
        log_info("Creating new server");
 
        if !settings.is_none() {
            let conf = settings.unwrap();

            self.bind_ip = conf.mud.bind_ip.clone();
            self.port = conf.mud.port;
            self.wizlocked = conf.mud.wizlocked;
            self.wizlock_reason = conf.mud.wizlock_reason.clone();
            self.settings = Some(conf.clone());
            self.connections.clear();
            self.wr_streams.clear();
            self.rd_streams.clear();
            self.rd_handles.clear();
            self.initialized = true;
        }
    }

    pub fn start_server(&mut self) -> Arc<RwLock<Option<TcpListener>>> {
        let mut listener = None;

        if !self.settings.is_none() {
            let addr = format!("{}:{}", self.bind_ip, self.port);
            let bind_addr: SocketAddr = addr.parse().unwrap_or_else(|e| panic!("Bind address {} is invalid: {:?}", addr, e));
            let socket;
            
            if bind_addr.is_ipv4() {
                socket = TcpSocket::new_v4().unwrap_or_else(|e| panic!("Couldn't create IPv4 socket: {:?}", e));
            } else if bind_addr.is_ipv6() {
                socket = TcpSocket::new_v6().unwrap_or_else(|e| panic!("Couldn't create IPv6 socket: {:?}", e));
            } else {
                panic!("We cannot determine if this is IPv4 or IPv6!: {}", addr);
            }

            log_info(&format!("Binding to {}", addr));
            let _ = socket.set_reuseaddr(true);
            let _ = socket.bind(bind_addr);
            listener = Some(socket.listen(1024).unwrap_or_else(|e| panic!("Could not listen on {}: {:?}", addr, e)));
        }
        return Arc::new(RwLock::new(listener));
    }

 
    pub async fn send_message(&mut self, message: NetworkMessage) {
        let msgdata = message.data.as_slice();
        let data_len = message.data.len();
        let disconnect: bool = data_len == 0;
        let wr_streams = &mut self.wr_streams;
        let rd_streams = &mut self.rd_streams;
        let addr = message.dest.clone();

        match wr_streams.get_mut(&addr) {
            Some(item) => {
                if disconnect {
                    log_info(&format!("Disconnecting {:?}", addr));
                    shutdown_stream(item).await;
                    let wr_stream = wr_streams.remove(&addr);
                    let rd_stream = rd_streams.remove(&addr);
                    drop(wr_stream);
                    drop(rd_stream);
                    self.connections.remove(&addr);
                } else {
                    log_info(&format!("Sending {} bytes of data to {:?}", data_len, addr));
                    write_message(item, msgdata).await;
                }
            },
            None => {},
        }
    }

    pub async fn receive_message(&mut self, message: NetworkMessage) {
        let connections = &mut self.connections;
        let data_len = message.data.len();
        let addr = message.dest.clone();

        log_info(&format!("Received {} bytes of data from {:?}", data_len, addr));
        match connections.get_mut(&addr) {
            Some(item) => {
                let mut sender = item.rxsender.clone();
                if !sender.is_none() {
                    let result = sender.as_mut().unwrap().clone().send(message.clone()).await;
                    if result.is_err() {
                        log_error(&format!("Error sending: {:?}", result.err().unwrap()));
                    }
                }
            },
            None => {},
        }
    }


    pub fn get_settings(&mut self) -> Option<Settings> {
        return self.settings.clone();
    }
}


async fn shutdown_stream(item: &mut Arc<RwLock<OwnedWriteHalf>>) {
    let mutex_clone = Arc::clone(item);
    let _ = { 
        let mut stream = mutex_clone.write().await;
        stream.shutdown().await 
    };
}

async fn write_message(item: &mut Arc<RwLock<OwnedWriteHalf>>, data: &[u8]) {
    let mutex_clone = Arc::clone(item);
    let _ = {
        let mut stream = mutex_clone.write().await;
        stream.write_all(data).await
    };
}

pub async fn do_server_thread(barrier: Arc<Barrier>, shutdown_barrier: Arc<Barrier>,
                              ctlsender: broadcast::Sender<ControlSignal>) {
    let mut shutdown = false;
    let mut initialized = false;
    let mut server = { Server::get(None).await.write().await.clone() };
    let mut listener = server.start_server().clone();
    let mut ctlqueue = ctlsender.subscribe();

    log_info("Starting server thread");

    let _ = barrier.wait().await;

    // Shared transmit queue (MUD -> player connection)
    let (mut txsender, mut txreceiver) = mpsc::channel::<NetworkMessage>(2048);

    // Setup receive queue (player connection -> MUD)
    let (rxsender, mut rxreceiver) = mpsc::channel::<NetworkMessage>(2048);
    
    while !initialized {
        let ctlmsg = ctlqueue.recv().await.unwrap();
        match ctlmsg {
            ControlSignal::Shutdown => shutdown = true,
            ControlSignal::Reconfigure(new_settings) => {
                {
                    server = Server::get(Some(new_settings)).await.write().await.clone();
                }
                listener = server.start_server().clone();
                initialized = true;
            },
        }           
    }

    while !shutdown {
        tokio::select! {
            v = ctlqueue.recv() => {
                match v.unwrap() {
                    ControlSignal::Shutdown => shutdown = true,
                    ControlSignal::Reconfigure(new_settings) => {
                        log_info("Reconfiguring server thread");
                        if  new_settings.mud.bind_ip != server.bind_ip || new_settings.mud.port != server.port {
                            for (_, mut connection) in server.connections.drain() {
                                connection.disconnect(format!("Server shutting down")).await;
                            }
                            task::yield_now().await;
                            txreceiver.close();
                           
                            let mut buffer = vec![];
                            while txreceiver.recv_many(&mut buffer, 100).await > 0 {
                                buffer.clear();
                            }

                            (txsender, txreceiver) = mpsc::channel::<NetworkMessage>(2048);
                            
                            {
                                server = Server::get(Some(new_settings)).await.write().await.clone();
                            }
                            listener = server.start_server().clone();
                        }
                    },
                };
            },
            v = accept_connection(&mut listener) => {
                let (stream, addr) = v.unwrap();
                let (rd_half, wr_half) = stream.into_split();
                let rd_stream = Arc::new(RwLock::new(rd_half));
                server.rd_streams.insert(addr, rd_stream.clone());
                server.wr_streams.insert(addr, Arc::new(RwLock::new(wr_half)));

                let rd_ctlrx = ctlsender.subscribe();
                let rd_dataqueue = rxsender.clone();
                let rd_handle = tokio::spawn(async move {
                    do_read_thread(rd_ctlrx, &rd_dataqueue, addr, rd_stream.clone()).await; 
                });
                server.rd_handles.insert(addr, Arc::new(RwLock::new(rd_handle)));

                let mut connection = Connection::new(&txsender, addr).await;
                connection.start_processing().await;
                server.connections.insert(addr, connection.clone());
                connection.send_line(&txsender, format!("Hi! $c020PWelcome$c0007 to $c000b{}", server.get_settings().unwrap().mud.name)).await;
            },
            v = txreceiver.recv() => {
                server.send_message(v.unwrap().clone()).await;
            },
            v = rxreceiver.recv() => {
                server.receive_message(v.unwrap().clone()).await;
            },
        }
    }

    log_info("Closing open connections");

    for (addr, mut connection) in server.connections.clone() {
        log_info(&format!("Closing connection from {:?}", addr));
        connection.disconnect("Server shutting down".to_string()).await;
    }

    txreceiver.close();

    let mut finished = false;
    while !finished {
        tokio::select! {
            v = txreceiver.recv() => {
                if v.is_none() {
                    finished = true;
                } else {
                    server.send_message(v.unwrap().clone()).await;
                }
            },
        };
    }

    log_info("Shutting down server thread");
    let _ = shutdown_barrier.wait().await;
}

async fn accept_connection(item: &mut Arc<RwLock<Option<TcpListener>>>) -> io::Result<(TcpStream, SocketAddr)> {
    let mutex_clone = Arc::clone(item);
    let result = {
        let listener = mutex_clone.write().await;
        let result = listener.as_ref().unwrap().accept().await;
        result
    };
    return result;
}

async fn do_read_thread(mut ctlqueue: broadcast::Receiver<ControlSignal>, dataqueue: &mpsc::Sender<NetworkMessage>, 
                        addr: SocketAddr, stream: Arc<RwLock<OwnedReadHalf>>) {
    let mut shutdown = false;
    let mut buffer = BytesMut::with_capacity(1024);
    let mut rd_stream = stream.write().await;

    while !shutdown {
        tokio::select! {
            v = ctlqueue.recv() => {
                match v.unwrap() {
                    ControlSignal::Shutdown => shutdown = true,
                    ControlSignal::Reconfigure(_) => {},
                };
            },
            v = rd_stream.read_buf(&mut buffer) => {
                if v.is_err() {
                    let err = v.err();
                    log_info(&format!("Error on read from {:?}: {:?}", addr, err));
                    shutdown = true;
                } else {
                    let bytes_read = v.unwrap();
                    if bytes_read == 0 {
                        shutdown = true;
                    } else {
                        let message = NetworkMessage {
                            dest: addr.clone(),
                            data: buffer[..].to_vec(),
                        };
                        let _ = dataqueue.send(message).await;
                    }

                    buffer.clear();
                }
            }
        }
    }

    let message = NetworkMessage {
        dest: addr.clone(),
        data: vec![],
    };
    let _ = dataqueue.send(message).await;
} 
