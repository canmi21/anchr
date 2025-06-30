/* src/quic/client.rs */

use crate::setup::config::Config;
use quinn::{ClientConfig, Connection, Endpoint};
use rustls::{ClientConfig as RustlsClientConfig, RootCertStore};
use std::fs::File;
use std::io::BufReader;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

async fn authenticate_with_server(
    connection: &Connection,
    token: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut send, mut recv) = connection.open_bi().await?;
    send.write_all(token.as_bytes()).await?;
    send.finish()?;

    let response = timeout(Duration::from_secs(10), recv.read_to_end(1024)).await??;
    let response_str = String::from_utf8(response)?;
    if response_str == "AUTH_SUCCESS" {
        println!("+ Authentication successful");
        Ok(())
    } else {
        Err(format!("Authentication failed: {}", response_str).into())
    }
}

async fn handle_connection(connection: Connection, token: String) {
    if let Err(e) = authenticate_with_server(&connection, &token).await {
        println!("! Authentication failed: {}", e);
        connection.close(1u32.into(), b"auth failed");
        return;
    }

    println!(
        "+ Connected and authenticated to {}",
        connection.remote_address()
    );

    tokio::spawn({
        let connection = connection.clone();
        async move {
            let mut counter = 0;
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                match connection.open_bi().await {
                    Ok((mut send, mut recv)) => {
                        let message = format!("Hello from client #{}", counter);
                        counter += 1;
                        if let Err(e) = send.write_all(message.as_bytes()).await {
                            println!("! Failed to send message: {}", e);
                            break;
                        }
                        if let Err(e) = send.finish() {
                            println!("! Failed to finish send stream: {}", e);
                            break;
                        }
                        match recv.read_to_end(1024).await {
                            Ok(response) => {
                                let response_str = String::from_utf8_lossy(&response);
                                println!("+ Server response: {}", response_str);
                            }
                            Err(e) => {
                                println!("! Failed to read response: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        println!("! Failed to open stream: {}", e);
                        break;
                    }
                }
            }
        }
    });

    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        if connection.close_reason().is_some() {
            println!("+ Connection closed");
            break;
        }
    }
}

pub async fn start_quic_client(cfg: Config) {
    let mut roots = RootCertStore::empty();
    let cert_file = File::open(&cfg.setup.certificate).expect("cannot open cert file");
    let mut reader = BufReader::new(cert_file);

    for cert_result in rustls_pemfile::certs(&mut reader) {
        let cert = cert_result.expect("failed to parse certificate");
        let der_bytes = cert.as_ref();
        let len = der_bytes.len();
        if len > 8 {
            let head = &der_bytes[..4];
            let tail = &der_bytes[len - 4..];
            let fingerprint_to_log = format!(
                "{:02X}:{:02X}:{:02X}:{:02X}:****:****:{:02X}:{:02X}:{:02X}:{:02X}",
                head[0], head[1], head[2], head[3], tail[0], tail[1], tail[2], tail[3]
            );
            println!("> Using root certificate (fingerprint): {}", fingerprint_to_log);
        }

        roots.add(cert).expect("failed to add cert to root store");
    }

    let tls_config = RustlsClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let client_config = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)
            .expect("failed to create QUIC client config"),
    ));

    let mut endpoint = Endpoint::client("[::]:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(client_config);

    let addr = format!("{}:{}", cfg.network.address, cfg.network.port);
    println!("> Trying to connect to {}", addr);

    // Mask the auth token for secure logging
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

    let remote_addr = addr.to_socket_addrs().unwrap().next().unwrap();

    let connecting = match endpoint.connect(remote_addr, "localhost") {
        Ok(c) => c,
        Err(e) => {
            println!("! Failed to start connection: {}", e);
            return;
        }
    };

    match timeout(Duration::from_secs(5), connecting).await {
        Ok(Ok(connection)) => {
            let token = cfg.setup.auth_token.clone();
            handle_connection(connection, token).await;
        }
        Ok(Err(e)) => {
            println!("! Connection error: {}", e);
        }
        Err(_) => {
            println!("! Connection timeout");
        }
    }
}