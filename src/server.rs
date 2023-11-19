extern crate tokio;

use crate::settings::Settings;
use crate::connection::Connection;
use crate::send_log;
use crate::ControlSignal;
use std::net::SocketAddr;
use tokio::net::{TcpSocket, TcpListener, TcpStream};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::task;
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct Message {
    pub dest: Vec<SocketAddr>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Server {
    bind_ip: String,
    port: u16,
    wizlocked: bool,
    wizlock_reason: String,
    aws_profile: String,
    settings: Option<Settings>,
    logqueue: mpsc::Sender<String>,
    pub connections: HashMap<SocketAddr, Connection>,
    pub streams: Rc<RefCell<HashMap<SocketAddr, TcpStream>>>,
}

impl Server {    
    fn new(logqueue: &mpsc::Sender<String>, settings: Option<Settings>) -> Result<Self, ()> {
        send_log(logqueue, "Creating new server");

        if settings.is_none() {
            let s = Server {
                bind_ip: "".to_string(),
                port: 0,
                wizlocked: false,
                wizlock_reason: "".to_string(),
                aws_profile: "".to_string(),
                settings: None,
                logqueue: logqueue.clone(),
                connections: HashMap::new(),
                streams: Rc::new(RefCell::new(HashMap::new())),
            };
            return Ok(s);
        }

        let conf = settings.unwrap();

        let s = Server {
            bind_ip: conf.mud.bind_ip.clone(),
            port: conf.mud.port,
            wizlocked: conf.mud.wizlocked,
            wizlock_reason: conf.mud.wizlock_reason.clone(),
            aws_profile: conf.mud.aws_profile.clone(),
            settings: Some(conf.clone()),
            logqueue: logqueue.clone(),
            connections: HashMap::new(),
            streams: Rc::new(RefCell::new(HashMap::new())),
        };

        return Ok(s);
    }

    pub fn start_server(&mut self) -> Rc<RefCell<Option<TcpListener>>> {
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

            send_log(&self.logqueue, &format!("Binding to {}", addr));
            let _ = socket.set_reuseaddr(true);
            let _ = socket.bind(bind_addr);
            listener = Some(socket.listen(1024).unwrap_or_else(|e| panic!("Could not listen on {}", addr)));
        }
        return Rc::new(RefCell::new(listener));
    }

    pub async fn send_message(&mut self, message: Message) {
        let msgdata = message.data.as_slice();
        let disconnect: bool = message.data.len() == 0;
        let mut streams = self.streams.borrow_mut();

        for addr in message.dest {
            match streams.get_mut(&addr) {
                Some(stream) => {
                    if disconnect {
                        let _ = (*stream).shutdown().await;
                        streams.remove(&addr);
                        self.connections.remove(&addr);
                    } else {
                        let _ = (*stream).write_all(msgdata).await;
                    }
                },
                None => {},
            }

        }
    }

}


pub async fn do_server_thread(ctlsender: broadcast::Sender<ControlSignal>, logqueue: &mpsc::Sender<String>) {
    let mut shutdown = false;
    let mut initialized = false;
    let mut server = Server::new(logqueue, None).unwrap();
    let mut listener_wrapped = server.start_server();
    let mut ctlqueue = ctlsender.subscribe();

    // Shared transmit queue (MUD -> player connection)
    let (mut txsender, mut txreceiver) = mpsc::channel::<Message>(2048);
    
    send_log(logqueue, "Starting server thread");

    while !initialized {
        let ctlmsg = ctlqueue.recv().await.unwrap();
        match ctlmsg {
            ControlSignal::Shutdown => shutdown = true,
            ControlSignal::Reconfigure(new_settings) => {
                server = Server::new(logqueue, Some(new_settings)).unwrap();
                listener_wrapped = server.start_server();
                initialized = true;
            },
        }           
    }

    while !shutdown {
        let listener = unsafe { (&*(Rc::as_ptr(&listener_wrapped))).as_ptr().as_ref() }.unwrap().as_ref().unwrap();

        tokio::select! {
            v = ctlqueue.recv() => {
                match v.unwrap() {
                    ControlSignal::Shutdown => shutdown = true,
                    ControlSignal::Reconfigure(new_settings) => {
                        send_log(logqueue, "Reconfiguring server thread");
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

                            (txsender, txreceiver) = mpsc::channel::<Message>(2048);
                            
                            server = Server::new(logqueue, Some(new_settings)).unwrap();
                            listener_wrapped = server.start_server();
                        }
                    },
                };
            },
            v = listener.accept() => {
                let (stream, addr) = v.unwrap();
                let connection = Connection::new(logqueue, &txsender, addr).unwrap();
                server.connections.insert(addr, connection);
                server.streams.borrow_mut().insert(addr, stream);
            },
            v = txreceiver.recv() => {
                server.send_message(v.unwrap().clone()).await;
            }
        }
    }
}
