/* src/quic/bootstrap.rs */

use crate::setup::config::Config;
use quinn::{Endpoint, ServerConfig, TransportConfig, Connection};
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

async fn handle_client_connection(connection: Connection, expected_token: String) {
    println!("+ New connection from {}", connection.remote_address());

    let mut auth_stream = match connection.accept_bi().await {
        Ok((send, recv)) => (send, recv),
        Err(e) => {
            println!("! Failed to accept auth stream: {}", e);
            connection.close(1u32.into(), b"auth stream failed");
            return;
        }
    };

    let token_data = match timeout(Duration::from_secs(10), auth_stream.1.read_to_end(1024)).await {
        Ok(Ok(data)) => data,
        Ok(Err(e)) => {
            println!("! Failed to read token: {}", e);
            connection.close(1u32.into(), b"token read failed");
            return;
        }
        Err(_) => {
            println!("! Token authentication timeout");
            connection.close(1u32.into(), b"auth timeout");
            return;
        }
    };

    let received_token = match String::from_utf8(token_data) {
        Ok(token) => token,
        Err(e) => {
            println!("! Invalid token format: {}", e);
            connection.close(1u32.into(), b"invalid token format");
            return;
        }
    };

    if received_token != expected_token {
        println!("! Invalid token from {}", connection.remote_address());
        let _ = auth_stream.0.write_all(b"AUTH_FAILED").await;
        let _ = auth_stream.0.finish();
        connection.close(1u32.into(), b"invalid token");
        return;
    }

    if let Err(e) = auth_stream.0.write_all(b"AUTH_SUCCESS").await {
        println!("! Failed to send auth response: {}", e);
        connection.close(1u32.into(), b"auth response failed");
        return;
    }

    if let Err(e) = auth_stream.0.finish() {
        println!("! Failed to finish auth stream: {}", e);
        connection.close(1u32.into(), b"auth stream finish failed");
        return;
    }

    println!("+ Client {} authenticated successfully", connection.remote_address());

    loop {
        match connection.accept_bi().await {
            Ok((mut send, mut recv)) => {
                tokio::spawn(async move {
                    match recv.read_to_end(1024 * 1024).await {
                        Ok(data) => {
                            println!("+ Received {} bytes", data.len());

                            if let Err(e) = send.write_all(&data).await {
                                println!("! Failed to echo data: {}", e);
                            } else {
                                let _ = send.finish();
                            }
                        }
                        Err(e) => {
                            println!("! Failed to read stream data: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                println!("+ Connection closed: {}", e);
                break;
            }
        }
    }
}

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
    println!("> Expected auth token: {}", cfg.setup.auth_token);

    while let Some(connecting) = endpoint.accept().await {
        let token = cfg.setup.auth_token.clone();
        tokio::spawn(async move {
            match connecting.await {
                Ok(connection) => {
                    handle_client_connection(connection, token).await;
                }
                Err(e) => {
                    println!("! Connection failed: {}", e);
                }
            }
        });
    }
}