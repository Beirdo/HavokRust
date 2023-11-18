extern crate directories;

use config::{Config, ConfigError, Environment, File};
use directories::ProjectDirs;
use serde_derive::Deserialize;
use std::env;
use std::fs;
use std::path::Path;
use tokio::sync::mpsc;
use super::send_log;

#[derive(Debug, Clone, Deserialize)]
pub struct Global {
    pub config_dir: String,
    pub data_dir: String,
    pub log_file: String,
    pub run_mode: String,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct Settings {
    pub debug: bool,
    pub global: Global,
}

impl Settings {
    pub fn new(appname: &str, logqueue: &mpsc::Sender<String>) -> Result<Self, ConfigError> {
        send_log(&logqueue, &format!("Loading settings for {}", appname));

        let run_mode = env::var("HAVOK_RUN_MODE").unwrap_or_else(|_| "development".into());

        let config_dir = if let Some(project_dirs) = ProjectDirs::from("net", "Beirdo", &appname) {
            let config_dir = project_dirs.config_dir().to_str().unwrap();
            String::from(config_dir)
        } else {
            String::from("config")
        };

        let data_dir = if let Some(project_dirs) = ProjectDirs::from("net", "Beirdo", &appname) {
            let data_dir = project_dirs.data_dir().to_str().unwrap();
            String::from(data_dir)
        } else {
            String::from("data")
        };

        let log_file = String::from(Path::new(&data_dir).join(format!("{}.log", appname)).to_str().unwrap());

        if !Path::new(&config_dir).is_dir() {
            send_log(&logqueue, &format!("Dir does not exist: {}", config_dir));
            fs::create_dir_all(&config_dir).unwrap_or_else(|e| {
                panic!("Problem creating directory {}: {:?}", config_dir, e);
            })
        }

        if !Path::new(&data_dir).is_dir() {
            send_log(&logqueue, &format!("Dir does not exist: {}", data_dir));
            fs::create_dir_all(&data_dir).unwrap_or_else(|e| {
                panic!("Problem creating directory {}: {:?}", data_dir, e);
            })
        }

        let s = Config::builder()
            // Set defaults
            .set_default("debug", false)?
            // Start off with merging in the "default" config file
            .add_source(File::with_name(&format!("{}/default", config_dir)).required(false))
            // Add in current environment file (defaulting to development)
            .add_source(File::with_name(&format!("{}/{}", config_dir, run_mode)).required(false))
            // Add in local file
            .add_source(File::with_name(&format!("{}/local", config_dir)).required(false))
            // Add in settings from environment prefixed with "HAVOK_"
            .add_source(Environment::with_prefix("havok"))
            .set_override("global.config_dir", config_dir)?
            .set_override("global.data_dir", data_dir)?
            .set_override("global.log_file", log_file)?
            .set_override("global.run_mode", run_mode)?
            .build()?;

        s.try_deserialize()
    }
}