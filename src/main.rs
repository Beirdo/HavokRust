#[macro_use] extern crate log;
extern crate tokio;

mod settings;
mod server;
mod connection;
mod logging;
mod ansicolors;
mod dnslookup;

use tokio::signal;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{broadcast, mpsc, Barrier};
use settings::Settings;
use server::{Server, do_server_thread};
use dnslookup::do_dns_lookup_thread;
use logging::*;
use std::sync::Arc;
use std::env;
use std::process;

use crate::ansicolors::AnsiColors;


#[derive(Debug, Clone)]
pub enum ControlSignal {
    Reconfigure(Settings),
    Shutdown,
}


#[tokio::main]
async fn main() {
    let mut shutdown = false;
    let appname: String = String::from("HavokMudRust");
    
    let (logtx, logrx) = mpsc::channel::<LogMessage>(256);

    send_log(&logtx, &format!("Starting {}", appname));

    AnsiColors::set_logqueue(&logtx);
    Server::set_logqueue(&logtx).await;

    let mut settings = Settings::new(&appname, &logtx).unwrap().clone();
    send_log(&logtx, &format!("Settings: {:?}", settings));

    let profile = settings.mud.aws_profile.clone();
    if profile.len() != 0 {
        let key = "AWS_PROFILE";
        env::set_var(key, profile);
    }

    let (ctltx, mut ctlrx) = broadcast::channel::<ControlSignal>(4);

    let thread_count = 6; // no barrier in Ctrl-C handler, but include the main thread
    let barrier = Arc::new(Barrier::new(thread_count - 1));
    let shutdown_barrier = Arc::new(Barrier::new(thread_count));

    let mut task_handle_list = vec![]; 

    // Start logging thread
    let log_barrier = barrier.clone();
    let log_shdn_barrier = shutdown_barrier.clone();
    let log_ctlrx = ctltx.subscribe();
    let log_logtx = logtx.clone();
    let log_handle = tokio::spawn(async move {
         do_log_thread(log_barrier, log_shdn_barrier, log_ctlrx, &log_logtx, logrx).await; 
    });
    send_log(&logtx, &format!("Log Thread: {:?}", log_handle));
    task_handle_list.push(log_handle);
    
    // Start Ctrl-C handler thread
    send_log(&logtx, "Starting Ctrl-C Handler thread");
    let ctrlc_ctltx = ctltx.clone();
    let ctrlc_handle = tokio::spawn(async move {
        signal::ctrl_c().await.unwrap();
        ctrlc_ctltx.send(ControlSignal::Shutdown.clone()).unwrap();
        let _ = tokio::spawn(async move {
            signal::ctrl_c().await.unwrap();
            process::exit(1);
        });
    });
    send_log(&logtx, &format!("Ctrl-C Thread: {:?}", ctrlc_handle));
    task_handle_list.push(ctrlc_handle);

    // Start SIGHUP handler thread
    let sighup_barrier = barrier.clone();
    let sighup_shdn_barrier = shutdown_barrier.clone();
    let sighup_ctltx = ctltx.clone();
    let mut sighup_ctlrx = ctltx.subscribe();
    let sighup_logtx = logtx.clone();
    let sighup_handle = tokio::spawn(async move {
        send_log(&sighup_logtx, "Starting SIGHUP Handler thread");
    
        let _ = sighup_barrier.wait().await;

        let mut stream = signal(SignalKind::hangup()).unwrap();
        let mut shutdown = false;

        while !shutdown {
            tokio::select! {
                v = sighup_ctlrx.recv() => {
                    match v.unwrap() {
                        ControlSignal::Shutdown => {
                            shutdown = true;
                        },
                        ControlSignal::Reconfigure(_) => {},
                    };
                }, 
                _ = stream.recv() => {
                    send_log(&sighup_logtx, "Recieved SIGHUP, reloading config");
                    let new_settings = Settings::new(&appname, &sighup_logtx).unwrap().clone();
                    let ctrlsignal = ControlSignal::Reconfigure(new_settings.clone());
                    sighup_ctltx.send(ctrlsignal.clone()).unwrap_or_else(|e| panic!("Error: {:?}", e));
                },
            }
        }
    
        send_log(&sighup_logtx, "Shutting down SIGHUP Handler thread");
        let _ = sighup_shdn_barrier.wait().await;
    });
    send_log(&logtx, &format!("SIGHUP Thread: {:?}", sighup_handle));
    task_handle_list.push(sighup_handle);

    // Now we need to start the server thread
    let server_barrier = barrier.clone();
    let server_shdn_barrier = shutdown_barrier.clone();
    let server_ctltx = ctltx.clone();
    let server_logtx = logtx.clone();
    let server_handle = tokio::spawn(async move {
        do_server_thread(server_barrier, server_shdn_barrier, server_ctltx, &server_logtx).await;
    });
    send_log(&logtx, &format!("Server Thread: {:?}", server_handle));
    task_handle_list.push(server_handle);

    // Start up the DNS Lookup thread
    let dns_barrier = barrier.clone();
    let dns_shdn_barrier = shutdown_barrier.clone();
    let dns_ctltx = ctltx.clone();
    let dns_logtx = logtx.clone();
    let dns_handle = tokio::spawn(async move {
        do_dns_lookup_thread(dns_barrier, dns_shdn_barrier, dns_ctltx, &dns_logtx).await;
    });
    send_log(&logtx, &format!("DNS Lookup Thread: {:?}", dns_handle));
    task_handle_list.push(dns_handle);

    // Now wait for all the barriers
    let _ = barrier.wait().await;

    // Send the settings to all threads that care.
    let ctrlsignal = ControlSignal::Reconfigure(settings.clone());
    ctltx.send(ctrlsignal.clone()).unwrap_or_else(|e| panic!("Error: {:?}", e));

    while !shutdown {
        let ctlmsg = ctlrx.recv().await.unwrap();
        match ctlmsg {
            ControlSignal::Shutdown => {
                shutdown = true;
            },
            ControlSignal::Reconfigure(new_settings) => {
                settings = new_settings.clone();
                send_log(&logtx, &format!("New Settings: {:?}", settings));
            },
        }
    } 

    // Account for any finished threads
    for handle in &task_handle_list {
        if handle.is_finished() {
            info!("Already finished {:?}", handle);
            let local_barrier = shutdown_barrier.clone();
            let _ = local_barrier.wait().await;
        }
    }


    let _ = shutdown_barrier.wait().await;

    // Shut down after all threads are done
    for handle in task_handle_list.drain(..) {
        info!("Waiting for {:?}", handle);
        if handle.await.is_err() {}
    }

    info!("Shutting down main thread");
}