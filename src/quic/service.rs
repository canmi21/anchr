/* src/quic/service.rs */

use crate::rfs::UploadMetadata;
use crate::setup::config::Config;
use crate::wsm::endpoints::{self, AuthState};
use crate::wsm::header::WsmHeader;
use quinn::{Connection, RecvStream, SendStream};
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{self, Duration};

pub type OngoingUploads = Arc<Mutex<HashMap<String, Arc<UploadMetadata>>>>;

#[derive(Clone)]
pub struct ServerState {
    pub ongoing_uploads: OngoingUploads,
}

pub async fn handle_connection(conn: Connection, cfg: Config) {
    println!("-> Handing connection from {} to service.", conn.remote_address());
    let auth_state = Arc::new(Mutex::new(AuthState::Unauthenticated));

    let server_state = ServerState {
        ongoing_uploads: Arc::new(Mutex::new(HashMap::new())),
    };

    // --- Step 1: Accept the main control stream FIRST ---
    let control_stream = match conn.accept_bi().await {
        Ok(stream) => stream,
        Err(e) => {
            println!("! Failed to accept the initial control stream: {}", e);
            return;
        }
    };
    let (mut control_send, mut control_recv) = control_stream;
    println!("  -- Control stream {} established.", control_send.id());

    // --- Step 2: NOW, spawn a task to handle all SUBSEQUENT worker streams ---
    let conn_clone = conn.clone();
    let cfg_clone = cfg.clone();
    let state_clone = server_state.clone();
    tokio::spawn(async move {
        loop {
            match conn_clone.accept_bi().await {
                Ok((send, recv)) => {
                    println!("  + Accepted a new worker stream.");
                    let worker_cfg = cfg_clone.clone();
                    let worker_state = state_clone.clone();
                    tokio::spawn(async move {
                        associate_and_run_worker(send, recv, worker_cfg, worker_state).await;
                    });
                }
                Err(e) => {
                    println!("! Error accepting worker stream: {}. Stopping worker listener.", e);
                    break;
                }
            }
        }
    });

    // --- Step 3: Proceed with handling the main control stream logic ---
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(32);
    let sender_task = tokio::spawn(async move {
        while let Some(msg_bytes) = rx.recv().await {
            if control_send.write_all(&msg_bytes).await.is_err() {
                break;
            }
        }
    });

    let mut header_buf = [0u8; 8];
    loop {
        match time::timeout(
            Duration::from_secs(15),
            control_recv.read_exact(&mut header_buf),
        )
        .await
        {
            Ok(Ok(())) => {
                let header = WsmHeader::from_bytes(&header_buf);
                if let ControlFlow::Break(_) = endpoints::dispatch_server(
                    &header,
                    &mut control_recv,
                    tx.clone(),
                    auth_state.clone(),
                    &cfg,
                    server_state.ongoing_uploads.clone(),
                )
                .await
                {
                    conn.close(2u32.into(), b"auth failure");
                    break;
                }
            }
            Ok(Err(e)) => {
                println!("! Read error on control stream: {}. Closing.", e);
                break;
            }
            Err(_) => {
                println!("! Control stream timeout. Closing connection.");
                conn.close(0u32.into(), b"keep-alive timeout");
                break;
            }
        }
    }
    sender_task.abort();

    println!("- Connection from {} closed.", conn.remote_address());
}

async fn associate_and_run_worker(
    send: SendStream,
    mut recv: RecvStream,
    cfg: Config,
    state: ServerState,
) {
    let mut header_buf = [0u8; 8];
    if time::timeout(Duration::from_secs(2), recv.read_exact(&mut header_buf))
        .await
        .is_err()
    {
        eprintln!("! Worker: Did not receive Hello in time.");
        return;
    }
    let header = WsmHeader::from_bytes(&header_buf);
    if header.opcode == 0x11 {
        // Worker Hello
        let mut payload = vec![0; header.payload_len as usize];
        if recv.read_exact(&mut payload).await.is_err() {
            return;
        }
        let file_hash = String::from_utf8_lossy(&payload).to_string();
        let uploads = state.ongoing_uploads.lock().await;
        if let Some(metadata) = uploads.get(&file_hash) {
            let metadata_clone = metadata.clone();
            drop(uploads);
            crate::rfs::worker::handle_worker_stream(send, recv, cfg, (*metadata_clone).clone())
                .await;
        } else {
            eprintln!("! Worker stream for unknown file hash: {}", file_hash);
        }
    } else {
        eprintln!(
            "! Worker stream's first message was not a Hello (0x11), but {:#02x}",
            header.opcode
        );
    }
}