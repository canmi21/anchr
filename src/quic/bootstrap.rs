/* src/quic/bootstrap.rs */

use crate::setup::config::Config;
use crate::quic::{auth, service};
use quinn::{ Endpoint, ServerConfig, TransportConfig};
use std::{fs::File, io::BufReader, net::SocketAddr, sync::Arc, time::Duration};

pub async fn start_quic_server(cfg: Config) {
    let certs = rustls_pemfile::certs(&mut BufReader::new(
        File::open(&cfg.setup.certificate).unwrap(),
    ))
    .map(|res| res.unwrap())
    .collect();

    let key = rustls_pemfile::private_key(&mut BufReader::new(
        File::open(&cfg.setup.private_key).unwrap(),
    ))
    .unwrap()
    .expect("Failed to find private key");

    let mut transport = TransportConfig::default();
    transport.max_concurrent_bidi_streams(10u32.into());
    transport.keep_alive_interval(Some(Duration::from_secs(5)));

    let mut server_config = ServerConfig::with_single_cert(certs, key).unwrap();
    server_config.transport = Arc::new(transport);

    let addr: SocketAddr = format!("{}:{}", cfg.network.listen, cfg.network.port)
        .parse()
        .unwrap();

    let endpoint = Endpoint::server(server_config, addr).unwrap();
    println!("> QUIC server running on {}", addr);

    let token_to_log = mask_token(&cfg.setup.auth_token);
    println!("> Expected auth token: {}", token_to_log);

    while let Some(connecting) = endpoint.accept().await {
        let expected_token = cfg.setup.auth_token.clone();
        tokio::spawn(async move {
            match connecting.await {
                Ok(conn) => {
                    println!("+ New connection from {}", conn.remote_address());
                    match auth::authenticate(&conn, &expected_token).await {
                        Ok(_) => {
                            println!("+ Auth success: {}", conn.remote_address());
                            service::handle_authenticated_client(conn).await;
                        }
                        Err(_) => {
                            println!("! Auth failed: {}", conn.remote_address());
                            conn.close(1u32.into(), b"unauthorized");
                        }
                    }
                }
                Err(e) => println!("! Connection failed: {}", e),
            }
        });
    }
}

fn mask_token(token: &str) -> String {
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
}
