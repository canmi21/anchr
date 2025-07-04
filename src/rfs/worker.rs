/* src/rfs/worker.rs */

use crate::rfs::{SharedUploadContext, UploadMetadata, UploadState};
use crate::setup::config::Config;
use crate::wsm::header::{PayloadType, WsmHeader, RESERVED_FINAL_FLAG};
use crate::wsm::msg_id;
use log::{error, info, warn};
use quinn::{RecvStream, SendStream};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::fs as tokio_fs; // Use Tokio's async filesystem module
use tokio::io::{AsyncReadExt, AsyncSeekExt}; // async IO traits
use tokio::sync::{mpsc, Mutex};

pub const CHUNK_SIZE: u64 = 512 * 1024;

type PendingChunkHashes = Arc<Mutex<HashMap<u64, [u8; 32]>>>;

// --- CLIENT-SIDE WORKER LOGIC ---

pub async fn run_worker_task(
    worker_id: u8,
    context: SharedUploadContext,
    mut send: SendStream,
    mut recv: RecvStream,
    main_tx: mpsc::Sender<Vec<u8>>,
) {
    info!("> Worker {} started.", worker_id);
    let (overall_hash, local_path, file_size) = {
        let ctx = context.lock().await;
        let c = ctx.as_ref().unwrap();
        (
            c.metadata.file_hash.clone(),
            c.local_file_path.clone(),
            c.metadata.file_size,
        )
    };

    let hello_header = WsmHeader::new(0x11, 0, PayloadType::Raw, overall_hash.len() as u32);
    let mut hello_msg = hello_header.to_bytes().to_vec();
    hello_msg.extend_from_slice(overall_hash.as_bytes());
    if send.write_all(&hello_msg).await.is_err() {
        error!("! Worker {}: Failed to send Hello message.", worker_id);
        return;
    }

    loop {
        let (chunk_id, total_chunks, completed_chunks) = {
            let mut ctx_lock = context.lock().await;
            if let Some(ctx) = ctx_lock.as_mut() {
                if let Some(id) = ctx.chunk_queue.lock().await.pop_front() {
                    (id, ctx.total_chunks, ctx.completed_chunks.clone())
                } else {
                    info!(
                        "> Worker {} found chunk queue empty, shutting down.",
                        worker_id
                    );
                    break;
                }
            } else {
                break;
            }
        };

        info!("> Worker {} picked up chunk #{}", worker_id, chunk_id);

        let chunk_data = match read_chunk(&local_path, chunk_id, file_size).await {
            Ok(data) => data,
            Err(e) => {
                error!(
                    "! Worker {}: Failed to read chunk {}: {}",
                    worker_id, chunk_id, e
                );
                continue;
            }
        };
        let chunk_hash: [u8; 32] = Sha256::digest(&chunk_data).into();

        let inquiry_header = WsmHeader::new(0x08, 0, PayloadType::Raw, 8 + 32);
        let mut inquiry_message = inquiry_header.to_bytes().to_vec();
        inquiry_message.extend_from_slice(&chunk_id.to_le_bytes());
        inquiry_message.extend_from_slice(&chunk_hash);

        if send.write_all(&inquiry_message).await.is_err() {
            continue;
        }

        let mut ack_header_buf = [0u8; 8];
        if recv.read_exact(&mut ack_header_buf).await.is_err() {
            continue;
        }
        let ack_header = WsmHeader::from_bytes(&ack_header_buf);
        if ack_header.opcode == 0x00 && ack_header.payload_len == 1 {
            let mut ack_payload = [0; 1];
            if recv.read_exact(&mut ack_payload).await.is_ok() {
                let mut chunk_successful = false;
                match ack_payload[0] {
                    1 => {
                        // Load
                        if send_chunk_data(&mut send, &mut recv, worker_id, chunk_id, &chunk_data)
                            .await
                        {
                            chunk_successful = true;
                        }
                    }
                    2 => {
                        // Skip
                        info!(
                            "> Worker {}: Server confirmed chunk #{} already exists. Skipping.",
                            worker_id, chunk_id
                        );
                        chunk_successful = true;
                    }
                    _ => warn!(
                        "! Worker {}: Received unknown ACK payload for chunk {}",
                        worker_id, chunk_id
                    ),
                }
                if chunk_successful {
                    check_and_finalize_upload(
                        completed_chunks,
                        total_chunks,
                        context.clone(),
                        main_tx.clone(),
                    )
                    .await;
                }
            }
        }
    }
}

async fn send_chunk_data(
    send: &mut SendStream,
    recv: &mut RecvStream,
    worker_id: u8,
    chunk_id: u64,
    chunk_data: &[u8],
) -> bool {
    let header = WsmHeader::new(0x09, 0, PayloadType::Raw, (8 + chunk_data.len()) as u32);
    let mut message = header.to_bytes().to_vec();
    message.extend_from_slice(&chunk_id.to_le_bytes());
    message.extend_from_slice(chunk_data);

    info!(
        "> Worker {}: Transferring chunk #{} ({} bytes)...",
        worker_id,
        chunk_id,
        chunk_data.len()
    );
    if send.write_all(&message).await.is_err() {
        return false;
    }

    let mut final_ack_buf = [0u8; 8];
    if recv.read_exact(&mut final_ack_buf).await.is_err() {
        return false;
    }
    let final_ack_header = WsmHeader::from_bytes(&final_ack_buf);
    if final_ack_header.is_final() {
        info!(
            "> Worker {}: Chunk #{} transferred successfully.",
            worker_id, chunk_id
        );
        true
    } else {
        warn!(
            "! Worker {}: Server requested reload for chunk #{}.",
            worker_id, chunk_id
        );
        false
    }
}

async fn read_chunk(file_path: &Path, chunk_id: u64, total_size: u64) -> std::io::Result<Vec<u8>> {
    let mut file = tokio_fs::File::open(file_path).await?;
    let offset = chunk_id * CHUNK_SIZE;
    file.seek(SeekFrom::Start(offset)).await?;
    let bytes_to_read = std::cmp::min(CHUNK_SIZE, total_size - offset) as usize;
    let mut buffer = vec![0; bytes_to_read];
    file.read_exact(&mut buffer).await?;
    Ok(buffer)
}

async fn check_and_finalize_upload(
    completed_chunks: Arc<AtomicU64>,
    total_chunks: u64,
    context: SharedUploadContext,
    tx: mpsc::Sender<Vec<u8>>,
) {
    let completed = completed_chunks.fetch_add(1, Ordering::SeqCst) + 1;
    log::info!("> {}/{} chunks completed.", completed, total_chunks);
    if completed >= total_chunks {
        info!("> All chunks transferred. Sending finalize request...");
        if let Some(ctx) = context.lock().await.as_mut() {
            if ctx.state == UploadState::Streaming {
                ctx.state = UploadState::Finishing;
                if let Some(msg_id) = msg_id::create_new_msg_id().await {
                    ctx.message_id = msg_id;
                    let payload = serde_json::to_string(&ctx.metadata).unwrap();
                    let header =
                        WsmHeader::new(0x10, msg_id, PayloadType::Json, payload.len() as u32);
                    let mut message = header.to_bytes().to_vec();
                    message.extend_from_slice(payload.as_bytes());
                    if tx.send(message).await.is_err() {
                        error!("! Failed to send finalize request.");
                    }
                }
            }
        }
    }
}

// --- SERVER-SIDE WORKER LOGIC ---

pub async fn handle_worker_stream(
    mut send: SendStream,
    mut recv: RecvStream,
    cfg: Config,
    upload_metadata: UploadMetadata,
) {
    let pending_hashes = PendingChunkHashes::default();
    let mut header_buf = [0u8; 8];
    loop {
        match recv.read_exact(&mut header_buf).await {
            Ok(()) => {
                let header = WsmHeader::from_bytes(&header_buf);
                match header.opcode {
                    0x08 => {
                        handle_chunk_inquiry(
                            &header,
                            &mut recv,
                            &mut send,
                            &cfg,
                            &upload_metadata,
                            pending_hashes.clone(),
                        )
                        .await
                    }
                    0x09 => {
                        handle_chunk_data(
                            &header,
                            &mut recv,
                            &mut send,
                            &cfg,
                            &upload_metadata,
                            pending_hashes.clone(),
                        )
                        .await
                    }
                    _ => {
                        eprintln!("! Worker: Unexpected opcode {:#04x}", header.opcode);
                        break;
                    }
                }
            }
            Err(quinn::ReadExactError::ReadError(quinn::ReadError::ConnectionLost(_))) => {
                if cfg.setup.log_level == "debug" {
                    println!("-> Worker: Connection lost gracefully.");
                }
                break;
            }
            Err(quinn::ReadExactError::FinishedEarly(_)) => {
                if cfg.setup.log_level == "debug" {
                    println!("-> Worker: Stream finished early.");
                }
                break;
            }
            Err(e) => {
                eprintln!("! Worker: Read error on stream: {}. Closing worker.", e);
                break;
            }
        }
    }
    if cfg.setup.log_level == "debug" {
        println!(
            "-> Worker stream finished for '{}'.",
            upload_metadata.file_name
        );
    }
}

async fn handle_chunk_inquiry(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: &mut SendStream,
    cfg: &Config,
    upload_metadata: &UploadMetadata,
    pending_hashes: PendingChunkHashes,
) {
    if header.payload_len != 40 {
        return;
    }
    let mut payload = vec![0; 40];
    if recv.read_exact(&mut payload).await.is_err() {
        return;
    }

    let chunk_id = u64::from_le_bytes(payload[0..8].try_into().unwrap());
    let client_hash: [u8; 32] = payload[8..40].try_into().unwrap();

    let base_path =
        crate::rfs::upload::resolve_and_validate_path(&upload_metadata.target_dir, cfg).unwrap();
    let tmp_dir_path = base_path.join(format!("{}.tmp", upload_metadata.file_name));
    let chunk_path = tmp_dir_path.join(format!("chunk_{}", chunk_id));

    let mut response_code: u8 = 1; // 1 = load
    let mut is_final = false;

    if tokio_fs::try_exists(&chunk_path).await.unwrap_or(false) {
        if let Ok(data) = tokio_fs::read(&chunk_path).await {
            if Sha256::digest(&data)[..] == client_hash {
                response_code = 2; // 2 = skip
                is_final = true;
            }
        }
    }

    if response_code == 1 {
        pending_hashes.lock().await.insert(chunk_id, client_hash);
    }

    let response_header = if is_final {
        WsmHeader::with_reserved(0x00, 0, PayloadType::Raw, 1, RESERVED_FINAL_FLAG)
    } else {
        WsmHeader::new(0x00, 0, PayloadType::Raw, 1)
    };
    let mut response = response_header.to_bytes().to_vec();
    response.push(response_code);
    let _ = tx.write_all(&response).await;
}

async fn handle_chunk_data(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: &mut SendStream,
    cfg: &Config,
    upload_metadata: &UploadMetadata,
    pending_hashes: PendingChunkHashes,
) {
    if header.payload_len <= 8 {
        return;
    }
    let mut payload = vec![0; header.payload_len as usize];
    if recv.read_exact(&mut payload).await.is_err() {
        return;
    }

    let chunk_id = u64::from_le_bytes(payload[0..8].try_into().unwrap());
    let chunk_data = &payload[8..];

    let expected_hash_opt = pending_hashes.lock().await.remove(&chunk_id);
    let mut is_final = false;

    if let Some(expected_hash) = expected_hash_opt {
        let received_hash: [u8; 32] = Sha256::digest(chunk_data).into();
        if received_hash == expected_hash {
            let base_path =
                crate::rfs::upload::resolve_and_validate_path(&upload_metadata.target_dir, cfg)
                    .unwrap();
            let tmp_dir_path = base_path.join(format!("{}.tmp", upload_metadata.file_name));
            let chunk_path = tmp_dir_path.join(format!("chunk_{}", chunk_id));
            if tokio_fs::write(chunk_path, chunk_data).await.is_ok() {
                is_final = true;
                if cfg.setup.log_level == "debug" {
                    println!("   - Worker: Saved chunk #{} successfully.", chunk_id);
                }
            } else {
                eprintln!("! Worker: Failed to write chunk #{} to disk.", chunk_id);
            }
        } else {
            eprintln!(
                "! Worker: Received chunk #{} with mismatched hash. Requesting reload.",
                chunk_id
            );
        }
    } else {
        eprintln!(
            "! Worker: Received chunk data for #{} without a pending hash. Ignoring.",
            chunk_id
        );
    }

    let response_header = WsmHeader::with_reserved(
        0x00,
        0,
        PayloadType::Raw,
        0,
        is_final.then_some(RESERVED_FINAL_FLAG).unwrap_or(0),
    );
    let _ = tx.write_all(&response_header.to_bytes()).await;
}