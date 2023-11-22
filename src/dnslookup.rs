extern crate tokio;

use std::net::*;
use tokio::sync::{broadcast, mpsc, Barrier, Mutex};
use hickory_resolver::Resolver;
use hickory_resolver::config::*;
use crate::logging::*;
use std::sync::Arc;
use crate::ControlSignal;
use tokio::time::{timeout, Duration};


#[derive(Debug, Clone)]
#[allow(unused)]
struct DnsItem {
    addr: IpAddr,
    names: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
#[allow(unused)]
struct DnsChannels {
    request_sender: Option<mpsc::Sender<DnsItem>>,
    response_sender: Option<broadcast::Sender<DnsItem>>,
}

use lazy_static::lazy_static;
lazy_static! {
    static ref DNS_CHANNELS: Arc<Mutex<DnsChannels>> = Arc::new(Mutex::new(DnsChannels {
        request_sender: None,
        response_sender: None,
    }));
}

#[allow(unused)]
pub async fn do_dns_lookup_thread(barrier: Arc<Barrier>, shutdown_barrier: Arc<Barrier>, ctlsender: broadcast::Sender<ControlSignal>,
                                  logqueue: &mpsc::Sender<LogMessage>) {
    send_log(&logqueue, "Starting DNS Lookup Thread");

    let mut shutdown = false;
    let mut ctlqueue = ctlsender.subscribe();
    let (request_sender, mut request_receiver) = mpsc::channel::<DnsItem>(256);
    let (response_sender, _response_receiver) = broadcast::channel::<DnsItem>(256);

    {
        let mut channels = DNS_CHANNELS.lock().await;
        channels.request_sender = Some(request_sender.clone());
        channels.response_sender = Some(response_sender.clone());
    }

    let resolver = Resolver::new(ResolverConfig::default(), ResolverOpts::default()).unwrap();

    let _ = barrier.wait().await;

    while !shutdown {
        tokio::select! {
            v = ctlqueue.recv() => {
                match v.unwrap() {
                    ControlSignal::Shutdown => {
                        shutdown = true;
                    },
                    ControlSignal::Reconfigure(_) => {},
                };
            }, 
            v = request_receiver.recv() => {
                let mut item = v.unwrap().clone();
                send_log(&logqueue, &format!("Received DNS request for {:?}", item.addr));
                let response = resolver.reverse_lookup(item.addr);
                if response.is_err() {
                    item.names = None;
                    send_error(&logqueue, &format!("DNS error looking up {:?}: {:?}", item.addr, response.err().unwrap()));
                } else {
                    let names: Vec<String> = response.unwrap().iter().map(|r| r.to_ascii()).collect();
                    if names.len() == 0 {
                        item.names = None;
                        send_log(&logqueue, &format!("No DNS results for {:?}", item.addr));
                    } else {
                        item.names = Some(names.clone());
                        send_log(&logqueue, &format!("Found DNS for {:?}: {:?}", item.addr, names));
                    }
                }
                let _ = request_sender.send(item);
            },
        }
    }

    send_log(&logqueue, "Shutting down DNS Lookup Thread");
    let _ = shutdown_barrier.wait().await;
}

#[allow(unused)]
pub async fn resolve_ip(addr: IpAddr) -> Option<Vec<String>> {
    let (request_sender, response_sender) = {
        let channels = DNS_CHANNELS.lock().await;
        (channels.request_sender.clone(), channels.response_sender.clone())
    };
    
    if request_sender.is_none() || response_sender.is_none() {
        return None;
    }

    let response_receiver = response_sender.unwrap().subscribe();

    let item = DnsItem {
        addr: addr,
        names: None,
    };
    let _ = request_sender.unwrap().send(item).await;

    let resp = timeout(Duration::from_secs(3), response_inner(addr, response_receiver)).await;
    if resp.is_err() {
        // Timed out
        return None;
    }

    return resp.unwrap().clone();
}

async fn response_inner(addr: IpAddr, mut response_receiver: broadcast::Receiver<DnsItem>) -> Option<Vec<String>> {
    loop {
        let resp = response_receiver.recv().await;
        if resp.is_err() {
            return None;
        } 
        
        let response = resp.unwrap();
        if response.addr == addr {
            return response.names.clone();
        }
    }
}