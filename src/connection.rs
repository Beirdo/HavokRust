use crate::server::Message;
use crate::logging::*;
use tokio::sync::mpsc;
use std::net::SocketAddr;

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Connection {
    txqueue: mpsc::Sender<Message>,
    addr: SocketAddr,
}

impl Connection {
    pub fn new(logqueue: &mpsc::Sender<LogMessage>, txsender: &mpsc::Sender<Message>, addr: SocketAddr) -> Result<Self, ()> {
        send_log(logqueue, &format!("New connection from {:?}", addr));

        let s = Connection {
            txqueue: txsender.clone(),
            addr: addr.clone(),
        };

        return Ok(s);
    }

    pub async fn disconnect(&mut self, reason: String) {
        self.send_message(reason.as_bytes()).await;
        self.send_message(b"").await;
    }

    pub async fn send_message(&mut self, message: &[u8]) {
        let mut msgvec = vec![];
        msgvec.extend_from_slice(message);
        let outmsg = Message {
            dest: vec![self.addr],
            data: msgvec,
        };
        let _ = self.txqueue.send(outmsg).await;
    }
}