extern crate tokio;

use simplelog::*;
use std::fs::OpenOptions;
use tokio::sync::{broadcast, mpsc, Barrier, RwLock};
use std::sync::Arc;
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


#[derive(Clone)]
#[allow(unused)]
pub struct Logging {
    initialized: bool,
    logtx: Option<mpsc::Sender<LogMessage>>,
    level: Level,
    logfile: Option<String>,
}

use lazy_static::lazy_static;
lazy_static! {
    static ref LOGGER: Arc<RwLock<Logging>> = Arc::new(RwLock::new(Logging {
        initialized: false,
        logtx: None,
        level: Level::Info,
        logfile: None,
    }));
}

impl Logging {
    pub async fn set_logqueue(logtx: &mpsc::Sender<LogMessage>) {
        let mut logger = LOGGER.write().await;

        logger.logtx = Some(logtx.clone());
    }

    pub async fn set_debug(debug: bool) {
        let mut logger = LOGGER.write().await;

        if debug {
            logger.level = Level::Debug;
        } else {
            logger.level = Level::Info;
        }
    }

    pub async fn set_logfile(logfile: String) {
        let mut logger = LOGGER.write().await;
        logger.logfile = Some(logfile.clone());
        logger.initialize();
    }

    fn initialize(&mut self) {
        self.level = Level::Info;

        if self.initialized {
            return;
        }

        let console_logger = TermLogger::new(
            LevelFilter::Debug,
            ConfigBuilder::new()
                .set_time_format_custom(format_description!("[hour]:[minute]:[second].[subsecond]"))
                .set_thread_level(LevelFilter::Trace)
                .set_target_level(LevelFilter::Trace)
                .set_location_level(LevelFilter::Trace)
                .set_thread_mode(ThreadLogMode::Both)
                .build(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        );

        let file_logger = WriteLogger::new(
            LevelFilter::Debug,
            ConfigBuilder::new()
                .set_time_format_custom(format_description!("[hour]:[minute]:[second].[subsecond]"))
                .set_thread_level(LevelFilter::Debug)
                .set_target_level(LevelFilter::Debug)
                .set_location_level(LevelFilter::Debug)
                .set_thread_mode(ThreadLogMode::Both)
                .build(),
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.logfile.as_ref().unwrap().clone())
                .unwrap(),
        );

        CombinedLogger::init(vec![
            console_logger,
            file_logger,
        ]).unwrap_or_else(|_| {});

        self.initialized = true;
    }
    
    pub async fn log_thread(& self, barrier: Arc<Barrier>, shutdown_barrier: Arc<Barrier>, ctltx: broadcast::Sender<ControlSignal>, 
                            mut logrx: mpsc::Receiver<LogMessage>) {
        let mut shutdown = false;
        let mut draining = false;
        let mut drained = false;
        let mut ctlqueue = ctltx.subscribe();

        self.log_info("Starting logging thread".to_string());

        let _ = barrier.wait().await;

        while (!shutdown || draining) && !drained {
            let mut a = None;
            let mut b = None;

            while a.is_none() && b.is_none() && !drained {
                tokio::select! {
                    v = logrx.recv() => {
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
                if log_enabled!(log_message.level) && log_message.level <= self.level {
                    log!(log_message.level, "{}", message);
                }
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
                        logrx.close();
                    },
                    ControlSignal::Reconfigure(_) => {},
                }
            }
        }

        drop(logrx);
        drop(ctlqueue);

        info!("Shutting down Logging thread");
    }

    #[allow(unused)]
    pub fn log_trace(& self, message: String) {
        if !self.logtx.is_none() {
            let logtx = self.logtx.as_ref().unwrap().clone();
            logtx.try_send(create_log_message(Level::Trace, &message)).unwrap();
        }
    }

    #[allow(unused)]
    pub fn log_debug(& self, message: String) {
        if !self.logtx.is_none() {
            let logtx = self.logtx.as_ref().unwrap().clone();
            logtx.try_send(create_log_message(Level::Debug, &message)).unwrap();
        }
    }

    #[allow(unused)]
    pub fn log_info(& self, message: String) {
        if !self.logtx.is_none() {
            let logtx = self.logtx.as_ref().unwrap().clone();
            logtx.try_send(create_log_message(Level::Info, &message)).unwrap();
        }
    }

    #[allow(unused)]
    pub fn log_warn(& self, message: String) {
        if !self.logtx.is_none() {
            let logtx = self.logtx.as_ref().unwrap().clone();
            logtx.try_send(create_log_message(Level::Warn, &message)).unwrap();
        }
    }

    #[allow(unused)]
    pub fn log_error(& self, message: String) {
        if !self.logtx.is_none() {
            let logtx = self.logtx.as_ref().clone().unwrap();
            logtx.try_send(create_log_message(Level::Error, &message)).unwrap();
        }
    }
}


pub async fn do_log_thread(barrier: Arc<Barrier>, shutdown_barrier: Arc<Barrier>, ctlqueue: broadcast::Sender<ControlSignal>, 
    logrx: mpsc::Receiver<LogMessage>) {

    let _ = tokio::spawn(async move {
        let logger = LOGGER.read().await;
        logger.log_thread(barrier, shutdown_barrier, ctlqueue, logrx).await;
    });
}

#[allow(unused)]
pub fn log_trace(message: &str) {
    let msg = String::from(message);
    let _ = tokio::spawn(async move {
        let mut logger = LOGGER.read().await;
        logger.log_trace(msg);    
    });
}

#[allow(unused)]
pub fn log_debug(message: &str) {
    let msg = String::from(message);
    let _ = tokio::spawn(async move {
        let mut logger = LOGGER.read().await;
        logger.log_debug(msg);    
    });
}

#[allow(unused)]
pub fn log_info(message: &str) {
    let msg = String::from(message);
    let _ = tokio::spawn(async move {
        let mut logger = LOGGER.read().await;
        logger.log_info(msg);   
    });
}

#[allow(unused)]
pub fn log_warn(message: &str) {
    let msg = String::from(message);
    let _ = tokio::spawn(async move {
        let mut logger = LOGGER.read().await;
        logger.log_warn(msg);    
    });
}

#[allow(unused)]
pub fn log_error(message: &str) {
    let msg = String::from(message);
    let _ = tokio::spawn(async move {
        let mut logger = LOGGER.read().await;
        logger.log_error(msg);
    });
}
