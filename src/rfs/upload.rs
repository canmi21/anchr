/* src/rfs/upload.rs */

use crate::rfs::{
    worker, PreparationResult, SharedUploadContext, UploadMetadata, UploadState,
    verify,
};
use crate::quic::service::OngoingUploads;
use crate::setup::config::Config;
use crate::wsm::header::{PayloadType, WsmHeader, RESERVED_FINAL_FLAG};
use crate::wsm::msg_id;
use log::{error, info};
use quinn::{Connection, RecvStream};
use std::sync::Arc;
use tokio::fs as tokio_fs;
use tokio::sync::mpsc;
use tokio::task;
use std::path::{Component, Path, PathBuf};

const MAX_WORKERS: u8 = 32;

pub fn calculate_workers(file_size: u64) -> u8 {
    if file_size == 0 {
        return 1;
    }
    let workers = (file_size as f64 / worker::CHUNK_SIZE as f64).ceil() as u8;
    workers.max(1).min(MAX_WORKERS)
}

// --- CLIENT-SIDE HANDLERS ---

pub async fn handle_init_ack(context: SharedUploadContext, tx: mpsc::Sender<Vec<u8>>) {
    let mut context_lock = context.lock().await;
    if let Some(ctx) = context_lock.as_mut() {
        if ctx.state != UploadState::Initiated {
            return;
        }
        let file_size = ctx.metadata.file_size;
        let num_workers = calculate_workers(file_size);
        info!(
            "> Upload acknowledged by server. Requesting {} worker stream(s)...",
            num_workers
        );
        if let Some(msg_id) = msg_id::create_new_msg_id().await {
            ctx.state = UploadState::WorkersOpening;
            ctx.message_id = msg_id;
            let header = WsmHeader::new(0x07, msg_id, PayloadType::Raw, 1);
            let mut message = header.to_bytes().to_vec();
            message.push(num_workers);
            if tx.send(message).await.is_err() {
                error!("! Failed to send worker request command.");
                msg_id::remove_msg_id(msg_id).await;
                *context_lock = None;
            }
        } else {
            error!("! Failed to get message ID for worker request.");
            *context_lock = None;
        }
    }
}

pub async fn handle_worker_ack(
    context: SharedUploadContext,
    connection: Arc<Connection>,
    main_tx: mpsc::Sender<Vec<u8>>,
) {
    let mut context_lock = context.lock().await;
    if let Some(ctx) = context_lock.as_mut() {
        if ctx.state != UploadState::WorkersOpening {
            return;
        }
        let num_workers = calculate_workers(ctx.metadata.file_size);
        let total_chunks =
            (ctx.metadata.file_size as f64 / worker::CHUNK_SIZE as f64).ceil() as u64;
        ctx.state = UploadState::Streaming;
        ctx.total_chunks = total_chunks;
        let mut queue = ctx.chunk_queue.lock().await;
        *queue = (0..total_chunks).collect();
        drop(queue);
        log::info!(
            "> Worker request approved. Spawning {} worker(s) for {} chunks...",
            num_workers,
            total_chunks
        );
        for i in 0..num_workers {
            let conn_clone = connection.clone();
            let upload_context = context.clone();
            let tx_clone = main_tx.clone();
            tokio::spawn(async move {
                match conn_clone.open_bi().await {
                    Ok((send, recv)) => {
                        log::info!("  - Worker stream {} opened successfully.", i + 1);
                        worker::run_worker_task(i + 1, upload_context, send, recv, tx_clone).await;
                    }
                    Err(e) => log::error!("! Failed to open worker stream {}: {}", i + 1, e),
                }
            });
        }
    }
}

// --- SERVER-SIDE HANDLERS ---

pub async fn handle_init_request(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
    cfg: &Config,
    ongoing_uploads: OngoingUploads,
) {
    if header.payload_len == 0 {
        eprintln!("! WSM-Server: Received upload request with no payload.");
        return;
    }
    let mut payload_buf = vec![0; header.payload_len as usize];
    if recv.read_exact(&mut payload_buf).await.is_err() {
        eprintln!("! WSM-Server: Failed to read upload metadata payload.");
        return;
    }
    match serde_json::from_slice::<UploadMetadata>(&payload_buf) {
        Ok(metadata) => {
            println!(
                "-> Received upload initiation for '{}'.",
                metadata.file_name
            );
            match prepare_upload_directory(&metadata, cfg).await {
                Ok(prep_result) => {
                    ongoing_uploads
                        .lock()
                        .await
                        .insert(metadata.file_hash.clone(), Arc::new(metadata));
                    let ack_code = match prep_result {
                        PreparationResult::New => 1,
                        PreparationResult::Resumable => 2,
                    };
                    let response_header =
                        WsmHeader::new(0x00, header.message_id, PayloadType::Raw, 1);
                    let mut response = response_header.to_bytes().to_vec();
                    response.push(ack_code);
                    if tx.send(response).await.is_err() {
                        eprintln!("! WSM-Server: Failed to send upload 'ACK' response.");
                    }
                }
                Err(e) => {
                    eprintln!(
                        "! WSM-Server: Failed to prepare upload for '{}': {}",
                        metadata.file_name, e
                    );
                }
            }
        }
        Err(e) => {
            eprintln!(
                "! WSM-Server: Failed to deserialize upload metadata: {}",
                e
            );
        }
    }
}

pub async fn handle_worker_request(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
    cfg: &Config,
) {
    if header.payload_len != 1 {
        eprintln!("! WSM-Server: Received worker request with invalid payload size.");
        return;
    }
    let mut payload_buf = [0; 1];
    if recv.read_exact(&mut payload_buf).await.is_err() {
        eprintln!("! WSM-Server: Failed to read worker request payload.");
        return;
    }
    if cfg.setup.log_level == "debug" {
        let num_workers = payload_buf[0];
        println!(
            "-> Received request to open {} worker stream(s).",
            num_workers
        );
    }
    let response_header = WsmHeader::new(0x00, header.message_id, PayloadType::Raw, 0);
    let response = response_header.to_bytes().to_vec();
    if tx.send(response).await.is_err() {
        eprintln!("! WSM-Server: Failed to send worker 'ACK' response.");
    }
}

pub async fn handle_finalize_request(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
    cfg: &Config,
    ongoing_uploads: OngoingUploads,
) {
    if header.payload_len == 0 {
        return;
    }
    let mut payload_buf = vec![0; header.payload_len as usize];
    if recv.read_exact(&mut payload_buf).await.is_err() {
        return;
    }

    match serde_json::from_slice::<UploadMetadata>(&payload_buf) {
        Ok(metadata) => {
            println!(
                "-> Received finalize request for '{}'. Spawning blocking task for assembly...",
                metadata.file_name
            );

            let meta_clone = metadata.clone();
            let cfg_clone = cfg.clone();
            let message_id = header.message_id;

            tokio::spawn(async move {
                let success = task::spawn_blocking(move || {
                    verify::assemble_and_verify_blocking(&meta_clone, &cfg_clone)
                })
                .await
                .unwrap_or(false);

                ongoing_uploads.lock().await.remove(&metadata.file_hash);

                let ack_code: u8 = if success { 1 } else { 0 };
                let response_header = WsmHeader::with_reserved(
                    0x00,
                    message_id,
                    PayloadType::Raw,
                    1,
                    RESERVED_FINAL_FLAG,
                );
                let mut response = response_header.to_bytes().to_vec();
                response.push(ack_code);
                if tx.send(response).await.is_err() {
                    eprintln!("! WSM-Server: Failed to send finalize 'ACK' response.");
                }
            });
        }
        Err(e) => {
            eprintln!(
                "! WSM-Server: Failed to deserialize finalize metadata: {}",
                e
            );
        }
    }
}

pub async fn prepare_upload_directory(
    metadata: &UploadMetadata,
    cfg: &Config,
) -> Result<PreparationResult, String> {
    let final_path = resolve_and_validate_path(&metadata.target_dir, cfg)?;
    let final_file_path = final_path.join(&metadata.file_name);
    let lock_filename = format!("{}.lock", metadata.file_name);
    let hash_filename = format!("{}.hash", metadata.file_name);
    let tmp_dir_name = format!("{}.tmp", metadata.file_name);
    let lock_file_path = final_path.join(lock_filename);
    let hash_file_path = final_path.join(hash_filename);
    let tmp_dir_path = final_path.join(tmp_dir_name);

    if tokio_fs::try_exists(&final_file_path)
        .await
        .unwrap_or(false)
    {
        return Err(format!(
            "File '{}' already exists at the target location.",
            metadata.file_name
        ));
    }
    if tokio_fs::try_exists(&lock_file_path)
        .await
        .unwrap_or(false)
    {
        if cfg.setup.log_level == "debug" {
            println!("   - Lock file found. Checking for resumable upload...");
        }
        let existing_hash = tokio_fs::read_to_string(&hash_file_path)
            .await
            .map_err(|e| format!("Failed to read existing hash file: {}", e))?;
        if existing_hash.trim() == metadata.file_hash {
            if cfg.setup.log_level == "debug" {
                println!("   - Hashes match. This is a resumable upload.");
            }
            return Ok(PreparationResult::Resumable);
        } else {
            if cfg.setup.log_level == "debug" {
                println!("   - Hashes do not match. Cleaning up stale upload files...");
            }
            tokio_fs::remove_file(&lock_file_path).await.ok();
            tokio_fs::remove_file(&hash_file_path).await.ok();
            if tokio_fs::try_exists(&tmp_dir_path).await.unwrap_or(false) {
                tokio_fs::remove_dir_all(&tmp_dir_path).await.ok();
            }
            if cfg.setup.log_level == "debug" {
                println!("   - Stale files cleaned up.");
            }
        }
    }

    if cfg.setup.log_level == "debug" {
        println!(
            "   - Preparing for new upload at: {}",
            final_path.to_string_lossy()
        );
    }
    tokio_fs::create_dir_all(&final_path)
        .await
        .map_err(|e| format!("Failed to create target directory: {}", e))?;
    tokio_fs::File::create(&lock_file_path)
        .await
        .map_err(|e| format!("Failed to create lock file: {}", e))?;
    tokio_fs::write(&hash_file_path, &metadata.file_hash)
        .await
        .map_err(|e| format!("Failed to create hash file: {}", e))?;
    tokio_fs::create_dir_all(&tmp_dir_path)
        .await
        .map_err(|e| format!("Failed to create .tmp directory: {}", e))?;
    if cfg.setup.log_level == "debug" {
        println!("   - Lock, hash, and tmp directory created successfully.");
    }
    Ok(PreparationResult::New)
}

pub fn resolve_and_validate_path(target_dir: &str, cfg: &Config) -> Result<PathBuf, String> {
    let virtual_path = Path::new(target_dir);
    let mut components = virtual_path.components();
    if components.next() != Some(Component::RootDir) {
        return Err(format!(
            "Invalid target_dir format: '{}'. Must start with /.",
            target_dir
        ));
    }
    let dev_name_comp = components
        .next()
        .ok_or_else(|| "target_dir is missing <dev_name>.".to_string())?;
    if !matches!(dev_name_comp, Component::Normal(_)) {
        return Err("Invalid <dev_name> in target_dir.".to_string());
    }
    let dev_name = dev_name_comp.as_os_str().to_string_lossy();
    let rfs_config = cfg
        .rfs
        .as_ref()
        .unwrap()
        .iter()
        .find(|v| v.dev_name == dev_name)
        .ok_or_else(|| format!("Device '{}' not found on server.", dev_name))?;
    let mut final_path = PathBuf::from(&rfs_config.bind_path);
    for component in components {
        match component {
            Component::Normal(name) => final_path.push(name),
            _ => {
                return Err(format!(
                    "Invalid path component in target_dir: '{}'.",
                    target_dir
                ));
            }
        }
    }
    Ok(final_path)
}