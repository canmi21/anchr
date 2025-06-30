// src/quic/client.rs

use crate::setup::config::Config;
use quinn::{ClientConfig, Endpoint};
use rustls::{ClientConfig as RustlsClientConfig, RootCertStore};
use rustls::pki_types::CertificateDer;
use std::fs::File;
use std::io::BufReader;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

pub async fn start_quic_client(cfg: Config) {
    let mut roots = RootCertStore::empty();
    let cert_file = File::open(&cfg.setup.certificate).expect("cannot open cert file");
    let mut reader = BufReader::new(cert_file);

    for cert_der_result in rustls_pemfile::certs(&mut reader) {
        let cert_der = cert_der_result.expect("failed to parse certificate");
        let cert = CertificateDer::from(cert_der);
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
            println!("+ Connected to {}", connection.remote_address());

            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
        Ok(Err(e)) => {
            println!("! Connection error: {}", e);
        }
        Err(_) => {
            println!("! Connection timeout");
        }
    }
}
