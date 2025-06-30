/* src/quic/bootstrap.rs */

use crate::setup::config::Config;
use quinn::{Endpoint, ServerConfig, TransportConfig};
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

pub async fn start_quic_server(cfg: Config) {
    let certs = rustls_pemfile::certs(&mut BufReader::new(
        File::open(&cfg.setup.certificate).unwrap(),
    ))
    .map(|result| result.unwrap())
    .collect();

    let key = rustls_pemfile::private_key(&mut BufReader::new(
        File::open(&cfg.setup.private_key).unwrap(),
    ))
    .unwrap()
    .expect("Failed to find private key in PEM file");

    let mut transport_config = TransportConfig::default();
    transport_config.max_concurrent_bidi_streams(10_u32.into());
    transport_config.keep_alive_interval(Some(Duration::from_secs(5)));

    let mut server_config = ServerConfig::with_single_cert(certs, key).unwrap();
    server_config.transport = Arc::new(transport_config);

    let addr: SocketAddr = format!("{}:{}", cfg.network.listen, cfg.network.port)
        .parse()
        .unwrap();

    let endpoint = Endpoint::server(server_config, addr).unwrap();
    println!("> QUIC server running on {}", addr);

    while let Some(connecting) = endpoint.accept().await {
        tokio::spawn(async move {
            match connecting.await {
                Ok(connection) => {
                    println!("+ New connection from {}", connection.remote_address());
                }
                Err(e) => {
                    println!("! Connection failed: {}", e);
                }
            }
        });
    }
}