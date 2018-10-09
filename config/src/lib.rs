//! configuration

#![deny(rust_2018_idioms)]
#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
// FIXME: doc the config
// #![deny(missing_docs)]

use std::fs::File;
use std::io::prelude::*;
use toml as Toml;

const CONFIG_FILE: &str = "witnet.toml";

#[macro_use]
extern crate serde_derive;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub logging: Option<LoggingConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub chain_type: Option<String>,
    pub db_root: Option<String>,
    pub host: String,
    pub p2p: Option<P2pConfig>,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct P2pConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    pub file_log_level: Option<String>,
    pub log_file_append: Option<bool>,
    pub log_file_path: Option<String>,
    pub log_to_file: Option<bool>,
    pub log_to_stdout: Option<bool>,
    pub stdout_log_level: Option<String>,
}

pub fn read_config() -> Option<Config> {
    let name: String = String::from(CONFIG_FILE);
    let mut input = String::new();

    File::open(&name)
        .and_then(|mut f| f.read_to_string(&mut input))
        .unwrap();

    match input.parse() {
        Ok(toml) => {
            let toml: Toml::Value = toml;
            let toml_str = toml.to_string();
            let decoded: Config = Toml::from_str(&toml_str).unwrap();
            Some(decoded)
        }
        Err(error) => {
            panic!("failed to parse TOML: {}", error);
        }
    }
}
