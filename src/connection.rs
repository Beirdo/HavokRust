extern crate tokio;

use crate::server::NetworkMessage;
use crate::logging::*;
use crate::ansicolors::AnsiColors;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use minijinja::{Environment, context};

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct UserMessage {
    bytes: Vec<u8>,
    string: String,
    jinja: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Connection {
    txqueue: mpsc::Sender<NetworkMessage>,
    addr: SocketAddr,
    ansi_mode: bool,
    ansi_colors: Arc<RwLock<AnsiColors>>,
    logqueue: mpsc::Sender<LogMessage>,
    rx_process_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    tx_process_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    disconnected: bool,
    pub rxsender: Option<mpsc::Sender<NetworkMessage>>,
    pub usertxsender: Option<broadcast::Sender<UserMessage>>,
    pub userrxsender: Option<broadcast::Sender<UserMessage>>,
}

impl Connection {
    pub fn new(logqueue: &mpsc::Sender<LogMessage>, txsender: &mpsc::Sender<NetworkMessage>, addr: SocketAddr) -> Self {
        send_log(logqueue, &format!("New connection from {:?}", addr));

        let s = Connection {
            txqueue: txsender.clone(),
            addr: addr.clone(),
            ansi_mode: true,
            ansi_colors: AnsiColors::get(),
            logqueue: logqueue.clone(),
            rx_process_handle: Arc::new(RwLock::new(None)),
            tx_process_handle: Arc::new(RwLock::new(None)),
            disconnected: false,
            rxsender: None,
            usertxsender: None,
            userrxsender: None,
        };

        return s;
    }

    #[allow(unused)]
    pub async fn start_processing(&mut self) {
        let (rxsender, mut rxreceiver) = mpsc::channel::<NetworkMessage>(256);
        self.rxsender = Some(rxsender);

        let (usertxsender, mut usertxreceiver) = broadcast::channel::<UserMessage>(256);
        let (userrxsender, mut userrxreceiver) = broadcast::channel::<UserMessage>(256);

        self.usertxsender = Some(usertxsender.clone());
        self.userrxsender = Some(userrxsender.clone());

        let mut rxconnection = self.clone();
        let rxlogqueue = self.logqueue.clone();
        let rxhandle = tokio::spawn(async move {
            rxconnection.do_rx_process_thread(&rxlogqueue, rxreceiver, userrxsender.clone()).await; 
        });
        self.rx_process_handle = Arc::new(RwLock::new(Some(rxhandle)));

        let mut txconnection = self.clone();
        let txlogqueue = self.logqueue.clone();
        let txsender = self.txqueue.clone();
        let txhandle = tokio::spawn(async move {
            txconnection.do_tx_process_thread(&txlogqueue, txsender.clone(), usertxsender.clone()).await; 
        });
        self.tx_process_handle = Arc::new(RwLock::new(Some(txhandle)));
    }


    async fn do_tx_process_thread(&mut self, logqueue: &mpsc::Sender<LogMessage>, txsender: mpsc::Sender<NetworkMessage>,
                                  usertxsender: broadcast::Sender<UserMessage>) {
        send_log(&logqueue, &format!("Starting Tx Process Thread for {:?}", self.addr));
        let mut usertxreceiver = usertxsender.subscribe();

        loop {
            let msg = usertxreceiver.recv().await.unwrap();
            if msg.string.len() == 0 && msg.bytes.len() == 0 && msg.jinja.is_none() {
                // diconnect user
                break;
            }

            if !msg.jinja.is_none() {
                let jinja_str: String = self.jinja_process(msg.jinja.unwrap());
                self.send_string(&txsender, jinja_str).await;
            } else if msg.bytes.len() != 0 {
                self.send_raw(&txsender, &msg.bytes).await;
            } else {
                self.send_string(&txsender, msg.string).await;
            }
        }

        send_log(&logqueue, &format!("Shutting down Tx Process Thread for {:?}", self.addr));
    }

    fn jinja_process(&mut self, mut jinjamap: HashMap<String, String>) -> String {
        let mut env = Environment::new();
        let template_str = jinjamap.remove("template").unwrap_or("".to_string());
        if template_str.len() == 0 {
            return "".to_string();
        }

        env.add_template("message", &template_str).unwrap();
        let tmpl = env.get_template("message").unwrap();
        let ctx = context!{jinjamap};
        let output_str = tmpl.render(ctx).unwrap();
        return output_str;
    }

    async fn do_rx_process_thread(&mut self, logqueue: &mpsc::Sender<LogMessage>, mut rxreceiver: mpsc::Receiver<NetworkMessage>,
                               userrxsender: broadcast::Sender<UserMessage>) {
        let mut incoming_buffer: Vec<u8> = Vec::new();

        send_log(&logqueue, &format!("Starting Rx Process Thread for {:?}", self.addr));

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
                let (mut linebuf, new_buffer) = self.read_line(incoming_buffer);
                incoming_buffer = new_buffer;
                
                if linebuf.len() == 0 {
                    break;
                }

                // send the line to the user channel
                linebuf.pop();     // strip \r
                linebuf.pop();     // strip \n
                send_log(&logqueue, &format!("Line Buffer: {:?}", linebuf));

                let line: String = String::from_utf8_lossy(&linebuf).to_string();
                send_log(&logqueue, &line);

                let usermsg = UserMessage {
                    bytes: linebuf.clone(),
                    string: line,
                    jinja: None,
                };
                let _ = userrxsender.send(usermsg);
            }
        }
        send_log(&logqueue, &format!("Shutting down Rx Process Thread for {:?}", self.addr));
    }

    pub async fn disconnect(&mut self, reason: String) {
        let txqueue = &self.txqueue.clone();
        self.send_line(txqueue, reason).await;
        self.send_raw(txqueue, b"").await;
        self.disconnected = true;
    }

    pub async fn send_raw(&mut self, txqueue: &mpsc::Sender<NetworkMessage>, message: &[u8]) {
        let mut msgvec = vec![];
        msgvec.extend_from_slice(message);
        let outmsg = NetworkMessage {
            dest: self.addr.clone(),
            data: msgvec,
        };
        let _ = txqueue.send(outmsg).await;
    }

    #[allow(unused)]
    pub async fn set_echo(&mut self, enable: bool) {
        let txqueue = &self.txqueue.clone();

        if enable {
            // IAC, WILL, TELOPT_ECHO
            self.send_raw(txqueue, &[0xFF, 0xFB, 0x01]).await;
        } else {
            // IAC, WONT, TELOPT_ECHO
            self.send_raw(txqueue, &[0xFF, 0xFC, 0x01]).await;
        }
    }

    #[allow(unused)]
    pub async fn send_string(&mut self, txqueue: &mpsc::Sender<NetworkMessage>, message: String) {
        let mut ansi_colors = self.ansi_colors.clone();
        let ansimsg = ansi_colors.read().unwrap().convert_string(message, self.ansi_mode);
        self.send_raw(txqueue, &ansimsg).await;
    }

    #[allow(unused)]
    pub async fn send_line(&mut self, txqueue: &mpsc::Sender<NetworkMessage>, message: String) {
        self.send_string(txqueue, message + "\r\n").await;
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

