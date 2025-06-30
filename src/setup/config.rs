/* src/setup/config.rs */

use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Clone)]
pub struct SetupConfig {
    pub mode: String,
    pub certificate: String,
    pub private_key: String,
    pub auth_token: String,
}

#[derive(Deserialize, Clone)]
pub struct NetworkConfig {
    pub listen: String,
    pub address: String,
    pub port: u16,
}

#[derive(Deserialize, Clone)]
pub struct Config {
    pub setup: SetupConfig,
    pub network: NetworkConfig,
}

impl Config {
    pub fn from_file(path: &str) -> Self {
        let content = fs::read_to_string(path).unwrap();
        toml::from_str(&content).unwrap()
    }
}