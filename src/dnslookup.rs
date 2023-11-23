extern crate tokio;

use std::net::*;
use tokio::sync::{broadcast, mpsc, Barrier, Mutex};
use hickory_resolver::TokioAsyncResolver;
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
    log_info(&logqueue, "Starting DNS Lookup Thread");

    let mut shutdown = false;
    let mut ctlqueue = ctlsender.subscribe();
    let (request_sender, mut request_receiver) = mpsc::channel::<DnsItem>(256);
    let (response_sender, _response_receiver) = broadcast::channel::<DnsItem>(256);

    {
        let mut channels = DNS_CHANNELS.lock().await;
        channels.request_sender = Some(request_sender.clone());
        channels.response_sender = Some(response_sender.clone());
    }

    let resolver = Arc::new(TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default()));

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
                log_info(&logqueue, &format!("Received DNS request for {:?}", item.addr));
                let query_logqueue = logqueue.clone();
                let query_resolver = resolver.clone();
                let query_sender = response_sender.clone();
                let handle = tokio::spawn(async move {
                    reverse_lookup(&query_logqueue, query_resolver, query_sender, item.addr).await
                });
            },
        }
    }

    log_info(&logqueue, "Shutting down DNS Lookup Thread");
    let _ = shutdown_barrier.wait().await;
}

async fn reverse_lookup(logqueue: &mpsc::Sender<LogMessage>, resolver: Arc<TokioAsyncResolver>, response_sender: broadcast::Sender<DnsItem>, addr: IpAddr) {
    let response = resolver.reverse_lookup(addr).await;
    let mut names = None;
    if response.is_err() {
        log_error(&logqueue, &format!("DNS error looking up {:?}: {:?}", addr, response.err().unwrap()));
    } else {
        let results: Vec<String> = response.unwrap().iter().map(|r| r.to_ascii()).collect();
        if results.len() == 0 {
            log_info(&logqueue, &format!("No DNS results for {:?}", addr));
        } else {
            names = Some(results.clone());
            log_info(&logqueue, &format!("Found DNS for {:?}: {:?}", addr, names));
        }
    }

    let item = DnsItem {
        addr: addr,
        names: names,
    };
    let _ = response_sender.send(item);
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
