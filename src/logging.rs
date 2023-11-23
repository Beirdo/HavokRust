extern crate tokio;

use simplelog::*;
use std::fs::OpenOptions;
use tokio::sync::{broadcast, mpsc, Barrier};
use std::sync::Arc;
use crate::settings::Settings;
use crate::ControlSignal;


#[derive(Debug, Clone)]
pub struct LogMessage {
    pub message: String,
    pub level: log::Level,
}

fn create_log_message(log_level: log::Level, msg: &str) -> LogMessage {
    LogMessage {
        message: String::from(msg),
        level: log_level,
    }
}

#[allow(unused)]
pub fn log_debug(logqueue: &mpsc::Sender<LogMessage>, message: &str) {
    logqueue.try_send(create_log_message(Level::Debug, message)).unwrap();
}

#[allow(unused)]
pub fn log_info(logqueue: &mpsc::Sender<LogMessage>, message: &str) {
    logqueue.try_send(create_log_message(Level::Info, message)).unwrap();
}

#[allow(unused)]
pub fn log_warn(logqueue: &mpsc::Sender<LogMessage>, message: &str) {
    logqueue.try_send(create_log_message(Level::Warn, message)).unwrap();
}

#[allow(unused)]
pub fn log_error(logqueue: &mpsc::Sender<LogMessage>, message: &str) {
    logqueue.try_send(create_log_message(Level::Error, message)).unwrap();
}

// TODO: make this actually work for reconfiguration
pub fn init_logging(settings: Settings) {
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

pub async fn do_log_thread(barrier: Arc<Barrier>, shutdown_barrier: Arc<Barrier>, mut ctlqueue: broadcast::Receiver<ControlSignal>, 
                           logtxqueue: &mpsc::Sender<LogMessage>, mut logqueue: mpsc::Receiver<LogMessage>) {
    let mut shutdown = false;
    let mut initialized = false;
    let mut draining = false;
    let mut drained = false;

    log_info(&logtxqueue, "Starting logging thread");

    let _ = barrier.wait().await;

    let mut settings: Settings;
    while !initialized {
        let ctlmsg = ctlqueue.recv().await.unwrap();
        match ctlmsg {
            ControlSignal::Shutdown => {
                shutdown = true;
                draining = true;
                drained = false;

                // Allow all other tasks to log dying gasps.
                shutdown_barrier.wait().await;

                info!("Draining logs");
                logqueue.close();
        },
            ControlSignal::Reconfigure(new_settings) => {
                log_info(logtxqueue, "Configuring logging thread");
                settings = new_settings.clone();
                init_logging(settings);
                initialized = true;
            },
        }
    }

    while (!shutdown || draining) && !drained {
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
            let log_message = a.unwrap();
            let message = log_message.message.to_owned();
            match log_message.level {
                Level::Trace => trace!("{}", message),
                Level::Debug => debug!("{}", message),
                Level::Info  => info!("{}", message),
                Level::Warn  => warn!("{}", message),
                Level::Error => error!("{}", message),
            };
        }

        if !b.is_none() {
            match b.unwrap() {
                ControlSignal::Shutdown => {
                    shutdown = true;
                    draining = true;
                    drained = false;

                    // Allow all other tasks to log dying gasps.
                    shutdown_barrier.wait().await;

                    info!("Draining logs");
                    logqueue.close();
                },
                ControlSignal::Reconfigure(new_settings) => {
                    log_info(logtxqueue, "Reconfiguring logging thread");
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

