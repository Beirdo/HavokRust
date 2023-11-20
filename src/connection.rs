use crate::server::Message;
use crate::logging::*;
use crate::ansicolors::AnsiColors;
use tokio::sync::mpsc;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Connection {
    txqueue: mpsc::Sender<Message>,
    addr: SocketAddr,
    ansi_mode: bool,
    ansi_colors: Arc<RwLock<AnsiColors>>,
    logqueue: mpsc::Sender<LogMessage>,
}

impl Connection {
    pub fn new(logqueue: &mpsc::Sender<LogMessage>, txsender: &mpsc::Sender<Message>, addr: SocketAddr) -> Result<Self, ()> {
        send_log(logqueue, &format!("New connection from {:?}", addr));

        let s = Connection {
            txqueue: txsender.clone(),
            addr: addr.clone(),
            ansi_mode: true,
            ansi_colors: AnsiColors::get(),
            logqueue: logqueue.clone(),
        };

        return Ok(s);
    }

    pub async fn disconnect(&mut self, reason: String) {
        self.send_line(reason).await;
        self.send_raw(b"").await;
    }

    pub async fn send_raw(&mut self, message: &[u8]) {
        let mut msgvec = vec![];
        msgvec.extend_from_slice(message);
        let outmsg = Message {
            dest: vec![self.addr],
            data: msgvec,
        };
        let _ = self.txqueue.send(outmsg).await;
    }

    #[allow(unused)]
    pub async fn set_echo(&mut self, enable: bool) {
        if enable {
            // IAC, WILL, TELOPT_ECHO
            self.send_raw(&[0xFF, 0xFB, 0x01]).await;
        } else {
            // IAC, WONT, TELOPT_ECHO
            self.send_raw(&[0xFF, 0xFC, 0x01]).await;
        }
    }

    #[allow(unused)]
    pub async fn send_string(&mut self, message: String) {
        let mut ansi_colors = self.ansi_colors.clone();
        let ansimsg = ansi_colors.read().unwrap().convert_string(message, self.ansi_mode);
        self.send_raw(&ansimsg).await;
    }

    #[allow(unused)]
    pub async fn send_line(&mut self, message: String) {
        self.send_string(message + "\r\n").await;
    }

    #[allow(unused)]
    pub async fn process_message(&mut self, data: &[u8]) {
        send_log(&self.logqueue, &format!("Received {} bytes from {:?}", data.len(), self.addr));
    }

}