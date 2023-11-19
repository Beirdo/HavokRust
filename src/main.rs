#[macro_use] extern crate log;
extern crate tokio;

mod settings;
mod server;
mod connection;

use simplelog::*;
use std::fs::OpenOptions;
use tokio::signal;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;
use tokio::sync::broadcast;
use tokio::task;
use settings::Settings;
use server::do_server_thread;


#[derive(Debug, Clone)]
pub enum ControlSignal {
    Reconfigure(Settings),
    Shutdown,
}


pub fn send_log(logqueue: &mpsc::Sender<String>, message: &str) {
    logqueue.try_send(String::from(message)).unwrap();
}

// TODO: make this actually work for reconfiguration
fn init_logging(settings: Settings) {
    let log_file = String::clone(&settings.global.log_file);

    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            ConfigBuilder::new()
                .set_thread_level(LevelFilter::Debug)
                .set_target_level(LevelFilter::Debug)
                .set_location_level(LevelFilter::Debug)
                .set_thread_mode(ThreadLogMode::Both)
                .build(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            ConfigBuilder::new()
                .set_thread_level(LevelFilter::Debug)
                .set_target_level(LevelFilter::Debug)
                .set_location_level(LevelFilter::Debug)
                .set_thread_mode(ThreadLogMode::Both)
                .build(),
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_file)
                .unwrap(),
        )
    ]).unwrap_or_else(|_| {});

}

async fn do_log_thread(mut ctlqueue: broadcast::Receiver<ControlSignal>, logtxqueue: &mpsc::Sender<String>, mut logqueue: mpsc::Receiver<String>) {
    let mut shutdown = false;
    let mut initialized = false;
    let mut draining = false;
    let mut drained = false;

    send_log(&logtxqueue, "Starting logging thread");

    let mut settings: Settings;
    while !initialized {
        let ctlmsg = ctlqueue.recv().await.unwrap();
        match ctlmsg {
            ControlSignal::Shutdown => shutdown = true,
            ControlSignal::Reconfigure(new_settings) => {
                send_log(logtxqueue, "Configuring logging thread");
                settings = new_settings.clone();
                init_logging(settings);
                initialized = true;
            },
        }
    }

    while !shutdown || draining {
        let mut a = None;
        let mut b = None;

        while a.is_none() && b.is_none() && !drained {
            tokio::select! {
                v = logqueue.recv() => {
                    if v.is_none() {
                        draining = false;
                        drained = true;
                        info!("Logs drained.");
                    } else {
                        a = Some(v.unwrap());
                    }
                }, 
                v = ctlqueue.recv() => b = Some(v.unwrap()),
            }
        }
        
        if !a.is_none() {
            info!("{}", a.unwrap().to_owned());
        }

        if !b.is_none() {
            match b.unwrap() {
                ControlSignal::Shutdown => {
                    shutdown = true;
                    draining = true;
                    drained = false;

                    // Allow all other tasks to log dying gasps.
                    task::yield_now().await;
                    info!("Draining logs");
                    logqueue.close();
                },
                ControlSignal::Reconfigure(new_settings) => {
                    send_log(logtxqueue, "Reconfiguring logging thread");
                    settings = new_settings.clone();
                    init_logging(settings);
                },
            }
        }
    }

    drop(logqueue);
    drop(ctlqueue);

    info!("Shutting down Logging thread");
}


#[tokio::main]
async fn main() {
    let mut shutdown = false;
    let appname: String = String::from("HavokMudRust");
    
    let (logtx, logrx) = mpsc::channel::<String>(256);

    send_log(&logtx, &format!("Starting {}", appname));

    let mut settings = Settings::new(&appname, &logtx).unwrap().clone();
    send_log(&logtx, &format!("Settings: {:?}", settings));

    let (ctltx, mut ctlrx) = broadcast::channel::<ControlSignal>(4);

    let mut task_handle_list = vec![]; 

    // Start logging thread
    let log_ctlrx = ctltx.subscribe();
    let log_logtx = logtx.clone();
    let log_handle = tokio::spawn(async move {
         do_log_thread(log_ctlrx, &log_logtx, logrx).await; 
    });
    task_handle_list.push(log_handle);
    
    // Start Ctrl-C handler thread
    send_log(&logtx, "Starting Ctrl-C Handler thread");
    let ctrlc_ctltx = ctltx.clone();
    let ctrlc_handle = tokio::spawn(async move {
        signal::ctrl_c().await.unwrap();
        ctrlc_ctltx.send(ControlSignal::Shutdown.clone()).unwrap();
    });
    task_handle_list.push(ctrlc_handle);

    // Start SIGHUP handler thread
    let sighup_ctltx = ctltx.clone();
    let mut sighup_ctlrx = ctltx.subscribe();
    let sighup_logtx = logtx.clone();
    let sighup_handle = tokio::spawn(async move {
        send_log(&sighup_logtx, "Starting SIGHUP Handler thread");
    
        let mut stream = signal(SignalKind::hangup()).unwrap();
        let mut shutdown = false;

        while !shutdown {
            tokio::select! {
                v = sighup_ctlrx.recv() => {
                    match v.unwrap() {
                        ControlSignal::Shutdown => shutdown = true,
                        ControlSignal::Reconfigure(_) => {},
                    };
                }, 
                _ = stream.recv() => {
                    send_log(&sighup_logtx, "Recieved SIGHUP, reloading config");
                    let new_settings = Settings::new(&appname, &logtx).unwrap().clone();
                    let ctrlsignal = ControlSignal::Reconfigure(new_settings.clone());
                    sighup_ctltx.send(ctrlsignal.clone()).unwrap_or_else(|e| panic!("Error: {:?}", e));
                },
            }
        }
    
        send_log(&sighup_logtx, "Shutting down SIGHUP Handler thread");
    });
    task_handle_list.push(sighup_handle);

    // Now we need to start the server thread
    let server_ctltx = ctltx.clone();
    let server_logtx = &logtx.clone();
    let server_handle = tokio::spawn(async move {
        do_server_thread(server_ctltx, server_logtx).await;
    });
    task_handle_list.push(server_handle);
    

    // Send the settings to all threads that care.
    let ctrlsignal = ControlSignal::Reconfigure(settings.clone());
    ctltx.send(ctrlsignal.clone()).unwrap_or_else(|e| panic!("Error: {:?}", e));

    while !shutdown {
        let ctlmsg = ctlrx.recv().await.unwrap();
        match ctlmsg {
            ControlSignal::Shutdown => shutdown = true,
            ControlSignal::Reconfigure(new_settings) => settings = new_settings.clone(),
        }
    } 

    // Shut down after all threads are done
    for handle in task_handle_list.drain(..) {
        if handle.await.is_err() {}
    }
}