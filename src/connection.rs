extern crate tokio;

use crate::server::Message;
use crate::logging::*;
use crate::ansicolors::AnsiColors;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
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
    rx_process_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    disconnected: bool,
    pub rxsender: Option<mpsc::Sender<Message>>,
}

impl Connection {
    pub fn new(logqueue: &mpsc::Sender<LogMessage>, txsender: &mpsc::Sender<Message>, addr: SocketAddr) -> Self {
        send_log(logqueue, &format!("New connection from {:?}", addr));

        let s = Connection {
            txqueue: txsender.clone(),
            addr: addr.clone(),
            ansi_mode: true,
            ansi_colors: AnsiColors::get(),
            logqueue: logqueue.clone(),
            rx_process_handle: Arc::new(RwLock::new(None)),
            disconnected: false,
            rxsender: None,
        };

        return s;
    }

    #[allow(unused)]
    pub async fn start_processing(&mut self) {
        let (rxsender, mut rxreceiver) = mpsc::channel::<Message>(256);
        self.rxsender = Some(rxsender);

        let mut connection = self.clone();
        let logqueue = self.logqueue.clone();
        let handle = tokio::spawn(async move {
            connection.do_process_thread(&logqueue, rxreceiver).await; 
        });
        self.rx_process_handle = Arc::new(RwLock::new(Some(handle)));
    }


    async fn do_process_thread(&mut self, logqueue: &mpsc::Sender<LogMessage>, mut rxreceiver: mpsc::Receiver<Message>) {
        let mut incoming_buffer: Vec<u8> = Vec::new();

        send_log(&logqueue, &format!("Starting Process Thread for {:?}", self.addr));

        while let Some(msg) = rxreceiver.recv().await {
            if msg.data.len() == 0 {
                self.disconnect("".to_string()).await;
                break;
            }

            send_log(&logqueue, &format!("Received {} bytes from {:?}", msg.data.len(), self.addr));
            send_log(&logqueue, &format!("Data: {:?}", msg.data));
            let mut data = self.handle_telnet_commands(msg.data);
            send_log(&logqueue, &format!("After telnet Data: {:?}", data));
            incoming_buffer.append(&mut data);

            loop {
                let (mut line, new_buffer) = self.read_line(incoming_buffer);
                incoming_buffer = new_buffer;
                
                if line.len() == 0 {
                    break;
                }

                // send the line to the user channel
                line.pop();     // strip \r
                line.pop();     // strip \n
                send_log(&logqueue, &format!("Line: {:?}", line));
                send_log(&logqueue, &String::from_utf8(line).unwrap_or("".to_string()));
            }
        }
        send_log(&logqueue, &format!("Shutting down Process Thread for {:?}", self.addr));
    }

    pub async fn disconnect(&mut self, reason: String) {
        self.send_line(reason).await;
        self.send_raw(b"").await;
        self.disconnected = true;
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

    /*
     * Commands defined in RFC854
     * Options defined in RFC855
     * 0xFF 0xF0      is "IAC SE" - end of subnegotiation
     * 0xFF 0xF1      is "IAC NOP"
     * 0xFF 0xF2      is "IAC DataMark" - should have TCP urgent
     * 0xFF 0xF3      is "IAC BRK" - break
     * 0xFF 0xF4      is "IAC IP" - interrupt process
     * 0xFF 0xF5      is "IAC AO" - abort output
     * 0xFF 0xF6      is "IAC AYT" - are you there
     * 0xFF 0xF7      is "IAC EC" - erase character
     * 0xFF 0xF8      is "IAC EL" - erase line
     * 0xFF 0xF9      is "IAC GA" - go ahead
     * 0xFF 0xFA      is "IAC SB" - start subnegotiation
     * 0xFF 0xFB 0xXX is "IAC WILL option"
     * 0xFF 0xFC 0xXX is "IAC WONT option"
     * 0xFF 0xFD 0xXX is "IAC DO option"
     * 0xFF 0xFE 0xXX is "IAC DONT option"
     * 0xFF 0xFF      is "IAC IAC" - send 0xFF
     */
    fn handle_telnet_commands(&mut self, data: Vec<u8>) -> Vec<u8> {
        let mut b: Vec<u8> = data.clone();
            
        loop {
            let mut iter = b.iter();
            let pos = iter.position(|&x| x == b'\xFF');
            if pos.is_none() {
                break;
            }

            let i = pos.unwrap();

            let command = iter.next();
            if command.is_none() {
                break;
            }

            let cmd: u8 = *command.unwrap();
            if cmd == b'\xFF' {
                //  We want to just leave the one 0xFF
                b.drain(i..i + 1);
            } else if cmd >= b'\xFB' {
                // These commands have a following option
                b.drain(i..i + 3);
            } else {
                b.drain(i..i + 2);
            }
        }

        return b;
    }

    fn read_line(&mut self, buffer: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
        let mut buf: Vec<u8> = buffer.clone();
        let s: Vec<u8> = buffer.clone();
        loop {
            // If there is a CRLF in the text, we have a full line to return to the caller.
            let mut i = 0;
            let mut found = false;
            let mut iter = s.iter().peekable();

            loop {
                let pos = iter.position(|&x| x == b'\r');
                if pos.is_none() {
                    break;
                }

                let nextvalue = iter.peek();
                if nextvalue.is_none() {
                    break;
                }

                if nextvalue.unwrap() == &&b'\n' {
                    i = pos.unwrap();
                    found = true;
                    break;
                } else if iter.next().is_none() {
                    break;
                }
            }

            if !found {
                return (vec![], buf);
            }

            let (line_data, new_buf) = s.split_at(i + 2);
            let mut line = line_data.to_vec();
            buf = new_buf.to_vec();
            
            // Process any backspaces
            loop {
                let s2 = line.clone();
                let mut iter2 = s2.iter();
        
                let pos = iter2.position(|&x| x == b'\x08');
                if pos.is_none() {
                    break;
                }
                
                let i = pos.unwrap();
                if i == 0 {
                    line.drain(..1);
                } else {
                    line.drain(i - 1..i + 1);
                }
            }

            return (line, buf);
        }
    }
}

