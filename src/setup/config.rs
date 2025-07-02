/* src/setup/config.rs */

use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Clone)]
pub struct RfsConfig {
    pub dev_name: String,
    pub bind_path: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SetupConfig {
    pub mode: String,
    pub certificate: String,
    pub private_key: String,
    pub auth_token: String,
    pub log_level: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NetworkConfig {
    pub listen: String,
    pub address: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub setup: SetupConfig,
    pub network: NetworkConfig,
    pub rfs: Option<Vec<RfsConfig>>,
}

impl Config {
    pub fn from_file(path: &str) -> Self {
        let content = fs::read_to_string(path).expect("Failed to read config file");
        toml::from_str(&content).expect("Failed to parse config file")
    }
}