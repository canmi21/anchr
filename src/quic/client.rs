/* src/quic/client.rs */

use crate::quic::keepalive;
use crate::setup::config::Config;
use quinn::{ClientConfig, Connection, ConnectionError, Endpoint};
use rustls::{ClientConfig as RustlsClientConfig, RootCertStore};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use tokio::time::Duration;

/// Main entry point for the client, now with an infinite reconnection loop.
pub async fn start_quic_client(cfg: Config) {
    loop {
        println!("> Attempting to connect to the server...");
        match connect_and_run(&cfg).await {
            Ok(_) => {
                println!("> Connection closed gracefully. Reconnecting in 3 seconds...");
            }
            Err(e) => {
                println!("! Disconnected: {}. Reconnecting in 3 seconds...", e);
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// Manages a single connection lifecycle: connects, authenticates, and handles keepalives.
async fn connect_and_run(cfg: &Config) -> Result<(), Box<dyn Error + Send + Sync>> {
    // --- 1. Setup Endpoint ---
    let mut roots = RootCertStore::empty();
    let cert_file = File::open(&cfg.setup.certificate)?;
    let mut reader = BufReader::new(cert_file);
    for cert_result in rustls_pemfile::certs(&mut reader) {
        let cert = cert_result?;
        let der_bytes = cert.as_ref();
        if der_bytes.len() > 8 {
            let head = &der_bytes[..4];
            let tail = &der_bytes[der_bytes.len() - 4..];
            println!(
                "> Using root certificate (fingerprint): {:02X?}..****..{:02X?}",
                head, tail
            );
        }
        roots.add(cert)?;
    }
    let tls_config = RustlsClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let client_config = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)?,
    ));
    let mut endpoint = Endpoint::client("[::]:0".parse()?)?;
    endpoint.set_default_client_config(client_config);

    // --- 2. Connect ---
    let addr_str = format!("{}:{}", cfg.network.address, cfg.network.port);
    let remote_addr: SocketAddr = addr_str.to_socket_addrs()?.next().ok_or("Invalid address")?;
    
    let token_to_log = {
        let token = &cfg.setup.auth_token;
        let len = token.chars().count();
        if len > 4 {
            format!(
                "{}{}{}",
                token.chars().next().unwrap(),
                "*".repeat(len - 4),
                token.chars().skip(len - 3).collect::<String>()
            )
        } else {
            token.to_string()
        }
    };
    println!("> Using auth token: {}", token_to_log);

    let connection = endpoint.connect(remote_addr, "localhost")?.await?;
    println!("+ Connection established with {}", connection.remote_address());

    // --- 3. Authenticate ---
    authenticate_with_server(&connection, &cfg.setup.auth_token).await?;

    // --- 4. Handle the active connection (including keepalives) ---
    handle_active_connection(connection).await
}

async fn authenticate_with_server(
    connection: &Connection,
    token: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (mut send, mut recv) = connection.open_bi().await?;
    send.write_all(token.as_bytes()).await?;
    send.finish()?;
    let response =
        tokio::time::timeout(Duration::from_secs(5), recv.read_to_end(1024)).await??;
    if response == b"AUTH_SUCCESS" {
        println!("+ Authentication successful");
        Ok(())
    } else {
        Err("Authentication failed".into())
    }
}

/// This function contains the keepalive loop. It runs until the connection is lost.
async fn handle_active_connection(
    connection: Connection,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut keepalive_interval =
        tokio::time::interval(Duration::from_secs(keepalive::KEEPALIVE_INTERVAL_SECS));

    loop {
        tokio::select! {
            _ = keepalive_interval.tick() => {
                if let Err(e) = send_keepalive_ping(&connection).await {
                    // Ping failed (timeout or other error), break the loop and signal disconnection.
                    return Err(format!("Keepalive failed: {}", e).into());
                }
            },
            // This branch detects if the connection was closed by the server or a fatal error occurred.
            event = connection.read_datagram() => {
                 match event {
                    Ok(_) => {}, // Ignore datagrams for now
                    Err(ConnectionError::LocallyClosed) => return Err("Connection closed by client".into()),
                    Err(e) => return Err(format!("Connection error: {}", e).into()),
                 }
            }
        }
    }
}

/// Sends one keepalive ping and waits for a response, with a timeout.
async fn send_keepalive_ping(
    connection: &Connection,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let ping_operation = async {
        let (mut send, mut recv) = connection.open_bi().await?;
        send.write_all(keepalive::KEEPALIVE_HEADER.as_bytes())
            .await?;
        send.finish()?;
        let response = recv.read_to_end(1024).await?;
        if response == keepalive::KEEPALIVE_ACK_HEADER.as_bytes() {
            Ok(())
        } else {
            Err("Invalid keepalive ACK".into())
        }
    };

    match tokio::time::timeout(Duration::from_millis(2500), ping_operation).await {
        Ok(Ok(_)) => Ok(()), // Ping-pong successful
        Ok(Err(e)) => Err(e), // Network or logic error during ping
        Err(_) => Err("Keepalive response timeout".into()), // Timeout waiting for pong
    }
}