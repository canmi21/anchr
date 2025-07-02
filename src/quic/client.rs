/* src/quic/client.rs */

use crate::console::app::Stats;
use crate::quic::keepalive;
use crate::setup::config::Config;
use crate::wsm::endpoints::{self, AuthState, InFlightPings};
use crate::wsm::header::{PayloadType, WsmHeader};
use crate::wsm::msg_id;
use log::{debug, error, info, warn};
use quinn::{ClientConfig, Endpoint};
use rustls::{ClientConfig as RustlsClientConfig, RootCertStore};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::net::{SocketAddr, ToSocketAddrs};
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time;

pub async fn run_network_tasks(
    cfg: Config,
    stats: Stats,
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
) {
    info!("Network task starting...");
    let mut first_failure_time: Option<Instant> = None;
    let stop_reconnecting = Arc::new(AtomicBool::new(false));

    let rx_arc = Arc::new(Mutex::new(rx));

    loop {
        if stop_reconnecting.load(Ordering::SeqCst) {
            error!("Halting reconnection attempts due to fatal error.");
            break;
        }

        info!("Attempting to connect to the server...");
        match connect_and_run(
            &cfg,
            stop_reconnecting.clone(),
            stats.clone(),
            tx.clone(),
            Arc::clone(&rx_arc),
        )
        .await
        {
            Ok(_) => {
                info!("Connection closed gracefully. Exiting network task.");
                break;
            }
            Err(e) => {
                if stop_reconnecting.load(Ordering::SeqCst) {
                    error!("Halting reconnection attempts due to fatal error: {}", e);
                    break;
                }
                warn!("Connection error: {}. Retrying in 3 seconds...", e);
                let now = Instant::now();
                let first_fail = first_failure_time.get_or_insert(now);
                if now.duration_since(*first_fail) > Duration::from_secs(30) {
                    // This is now a backup clear; the primary clear is on successful auth.
                    msg_id::drain_msg_id_pool().await;
                    first_failure_time = None;
                }
                time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

async fn connect_and_run(
    cfg: &Config,
    stop_reconnecting: Arc<AtomicBool>,
    stats: Stats,
    tx: mpsc::Sender<Vec<u8>>,
    rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut roots = RootCertStore::empty();
    let cert_file = File::open(&cfg.setup.certificate)?;
    let mut reader = BufReader::new(cert_file);
    for cert_result in rustls_pemfile::certs(&mut reader) {
        let cert = cert_result?;
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

    let addr_str = format!("{}:{}", cfg.network.address, cfg.network.port);
    let remote_addr: SocketAddr = addr_str.to_socket_addrs()?.next().ok_or("Invalid address")?;

    let connection = endpoint.connect(remote_addr, "localhost")?.await?;
    info!("Connection established with {}", connection.remote_address());

    let (mut control_send, mut control_recv) = connection.open_bi().await?;
    info!("Control stream opened for bidirectional communication.");

    let in_flight_pings: InFlightPings = Arc::new(Mutex::new(HashMap::new()));
    let auth_state = Arc::new(Mutex::new(AuthState::Unauthenticated));

    let stats_for_sender = stats.clone();
    tokio::spawn(async move {
        while let Some(msg_bytes) = rx.lock().await.recv().await {
            if let Err(e) = control_send.write_all(&msg_bytes).await {
                error!("Client failed to send message: {}", e);
                break;
            }
            stats_for_sender
                .tx_bytes
                .fetch_add(msg_bytes.len() as u64, Ordering::Relaxed);
        }
    });

    // --- Authentication Phase ---
    let auth_token = cfg.setup.auth_token.as_bytes();
    let auth_header = WsmHeader::new(
        0x03,
        msg_id::create_new_msg_id().await.unwrap_or(0),
        PayloadType::Raw,
        auth_token.len() as u32,
    );
    let mut auth_request = auth_header.to_bytes().to_vec();
    auth_request.extend_from_slice(auth_token);
    info!("Sending authentication request...");
    tx.send(auth_request).await?;

    let mut header_buf = [0u8; 8];
    match control_recv.read_exact(&mut header_buf).await {
        Ok(()) => {
            stats.rx_bytes.fetch_add(8, Ordering::Relaxed);
            let header = WsmHeader::from_bytes(&header_buf);
            stats
                .last_msg_id
                .store(header.message_id, Ordering::Relaxed);
            if let ControlFlow::Break(_) = endpoints::dispatch_client(
                &header,
                &mut control_recv,
                in_flight_pings.clone(),
                auth_state.clone(),
                stop_reconnecting.clone(),
                cfg,
                stats.clone(),
            ).await {
                error!("Dispatcher requested termination (auth failure).");
                return Err("Authentication failed".into());
            }
        }
        Err(e) => {
            warn!("Client connection lost during auth: {}. Triggering reconnect...", e);
            return Err(Box::new(e));
        }
    };

    // Check auth state after handling the response
    if *auth_state.lock().await != AuthState::Authenticated {
        return Err("Authentication was not successful.".into());
    }

    // --- Post-Authentication Phase ---
    info!("Clearing message pool for new session...");
    msg_id::drain_msg_id_pool().await;

    info!("Spawning keep-alive tasks...");
    let ping_tx = tx.clone();
    let pings_to_track = in_flight_pings.clone();
    let log_cfg = cfg.clone();
    let pinger_handle: JoinHandle<()> = tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_secs(1)).await;
            if let Some(ping_msg) = keepalive::build_client_ping().await {
                let msg_id = ping_msg[1];
                if log_cfg.setup.log_level == "debug" {
                    debug!("Queueing keep-alive PING (id: {})", msg_id);
                }
                pings_to_track.lock().await.insert(msg_id, Instant::now());
                if let Err(e) = ping_tx.send(ping_msg.to_vec()).await {
                    error!("Failed to queue PING: {}", e);
                    break;
                }
            } else {
                error!("Failed to create PING: message ID pool is full.");
            }
        }
    });

    let pings_to_watch = in_flight_pings.clone();
    let conn_for_timeout = connection.clone();
    let watcher_handle: JoinHandle<()> = tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_millis(100)).await;
            let mut timed_out = None;
            let mut pings = pings_to_watch.lock().await;
            for (msg_id, sent_at) in pings.iter() {
                if sent_at.elapsed() > Duration::from_millis(500) {
                    timed_out = Some(*msg_id);
                    break;
                }
            }
            if let Some(msg_id) = timed_out {
                warn!("PONG for msg_id {} not received in 500ms. Closing connection.", msg_id);
                conn_for_timeout.close(1u32.into(), b"PONG timeout");
                pings.clear();
                break;
            }
        }
    });
    
    info!("Client is now in main loop, handling PONGs...");
    let loop_result: Result<(), Box<dyn Error + Send + Sync>> = loop {
        match control_recv.read_exact(&mut header_buf).await {
            Ok(()) => {
                stats.rx_bytes.fetch_add(8, Ordering::Relaxed);
                let header = WsmHeader::from_bytes(&header_buf);
                stats
                    .last_msg_id
                    .store(header.message_id, Ordering::Relaxed);
                if let ControlFlow::Break(_) = endpoints::dispatch_client(
                    &header,
                    &mut control_recv,
                    in_flight_pings.clone(),
                    auth_state.clone(),
                    stop_reconnecting.clone(),
                    cfg,
                    stats.clone(),
                ).await {
                    error!("Dispatcher requested termination post-auth.");
                    break Err("Connection terminated by dispatcher".into());
                }
            }
            Err(e) => {
                warn!("Client connection lost: {}. Triggering reconnect...", e);
                break Err(Box::new(e));
            }
        }
    };

    pinger_handle.abort();
    watcher_handle.abort();
    loop_result
}