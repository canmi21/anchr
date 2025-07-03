/* src/rfs/upload.rs */

use crate::rfs::{PreparationResult, SharedUploadContext, UploadMetadata, UploadState};
use crate::setup::config::Config;
use crate::wsm::header::{PayloadType, WsmHeader};
use crate::wsm::msg_id;
use log::{error, info};
use quinn::{Connection, RecvStream};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

const ONE_MB: u64 = 1024 * 1024;
const MAX_WORKERS: u8 = 16;
pub fn calculate_workers(file_size: u64) -> u8 {
    if file_size == 0 { return 1; }
    let workers = (file_size as f64 / ONE_MB as f64).ceil() as u8;
    workers.max(1).min(MAX_WORKERS)
}
pub fn prepare_upload_directory(metadata: &UploadMetadata, cfg: &Config) -> Result<PreparationResult, String> {
    let final_path = resolve_and_validate_path(&metadata.target_dir, cfg)?;
    let final_file_path = final_path.join(&metadata.file_name);
    let lock_filename = format!("{}.lock", metadata.file_name);
    let hash_filename = format!("{}.hash", metadata.file_name);
    let tmp_dir_name = format!("{}.tmp", metadata.file_name);
    let lock_file_path = final_path.join(lock_filename);
    let hash_file_path = final_path.join(hash_filename);
    let tmp_dir_path = final_path.join(tmp_dir_name);
    if final_file_path.exists() {
        return Err(format!("File '{}' already exists at the target location.", metadata.file_name));
    }
    if lock_file_path.exists() {
        println!("   - Lock file found. Checking for resumable upload...");
        let existing_hash = fs::read_to_string(&hash_file_path).map_err(|e| format!("Failed to read existing hash file: {}", e))?;
        if existing_hash.trim() == metadata.file_hash {
            println!("   - Hashes match. This is a resumable upload.");
            return Ok(PreparationResult::Resumable);
        } else {
            println!("   - Hashes do not match. Cleaning up stale upload files...");
            let _ = fs::remove_file(&lock_file_path);
            let _ = fs::remove_file(&hash_file_path);
            if tmp_dir_path.exists() {
                let _ = fs::remove_dir_all(&tmp_dir_path);
            }
            println!("   - Stale files cleaned up.");
        }
    }
    println!("   - Preparing for new upload at: {}", final_path.to_string_lossy());
    fs::create_dir_all(&final_path).map_err(|e| format!("Failed to create target directory: {}", e))?;
    fs::File::create(&lock_file_path).map_err(|e| format!("Failed to create lock file: {}", e))?;
    fs::write(&hash_file_path, &metadata.file_hash).map_err(|e| format!("Failed to create hash file: {}", e))?;
    fs::create_dir_all(&tmp_dir_path).map_err(|e| format!("Failed to create .tmp directory: {}", e))?;
    println!("   - Lock, hash, and tmp directory created successfully.");
    Ok(PreparationResult::New)
}
fn resolve_and_validate_path(target_dir: &str, cfg: &Config) -> Result<PathBuf, String> {
    let virtual_path = Path::new(target_dir);
    let mut components = virtual_path.components();
    if components.next() != Some(Component::RootDir) { return Err(format!("Invalid target_dir format: '{}'. Must start with /.", target_dir)); }
    let dev_name_comp = components.next().ok_or_else(|| "target_dir is missing <dev_name>.".to_string())?;
    if !matches!(dev_name_comp, Component::Normal(_)) { return Err("Invalid <dev_name> in target_dir.".to_string()); }
    let dev_name = dev_name_comp.as_os_str().to_string_lossy();
    let rfs_config = cfg.rfs.as_ref().unwrap().iter().find(|v| v.dev_name == dev_name).ok_or_else(|| format!("Device '{}' not found on server.", dev_name))?;
    let mut final_path = PathBuf::from(&rfs_config.bind_path);
    for component in components {
        match component {
            Component::Normal(name) => final_path.push(name),
            _ => return Err(format!("Invalid path component in target_dir: '{}'.", target_dir)),
        }
    }
    Ok(final_path)
}


// --- Handler Functions ---

// [SERVER-SIDE] Handles the upload initiation request (0x06).
pub async fn handle_init_request(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
    cfg: &Config,
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
            println!("-> Received upload initiation request (id: {}):", header.message_id);
            match prepare_upload_directory(&metadata, cfg) {
                Ok(prep_result) => {
                    let ack_code = match prep_result {
                        PreparationResult::New => 1,
                        PreparationResult::Resumable => 2,
                    };
                    let response_header = WsmHeader::new(0x00, header.message_id, PayloadType::Raw, 1);
                    let mut response = response_header.to_bytes().to_vec();
                    response.push(ack_code);
                    if tx.send(response).await.is_err() {
                        eprintln!("! WSM-Server: Failed to send upload 'ACK' response.");
                    }
                }
                Err(e) => {
                    eprintln!("! WSM-Server: Failed to prepare upload for id {}: {}", header.message_id, e);
                }
            }
        }
        Err(e) => {
            eprintln!("! WSM-Server: Failed to deserialize upload metadata: {}", e);
        }
    }
}

// [SERVER-SIDE] Handles the worker stream request (0x07).
pub async fn handle_worker_request(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
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
    let num_workers = payload_buf[0];
    println!("-> Received request to open {} worker stream(s).", num_workers);

    let response_header = WsmHeader::new(0x00, header.message_id, PayloadType::Raw, 0);
    let response = response_header.to_bytes().to_vec();
    if tx.send(response).await.is_err() {
        eprintln!("! WSM-Server: Failed to send worker 'ACK' response.");
    }
}

// [CLIENT-SIDE] Handles the ACK for the upload initiation (the first 0x00 reply).
pub async fn handle_init_ack(context: SharedUploadContext, tx: mpsc::Sender<Vec<u8>>) {
    let mut context_lock = context.lock().await;
    if let Some(ctx) = context_lock.as_mut() {
        if ctx.state != UploadState::Initiated { return; }

        let file_size = ctx.metadata.file_size;
        let num_workers = calculate_workers(file_size);
        info!("> Upload acknowledged by server. Requesting {} worker stream(s)...", num_workers);

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

// [CLIENT-SIDE] Handles the ACK for the worker request (the second 0x00 reply).
pub async fn handle_worker_ack(context: SharedUploadContext, connection: Arc<Connection>) {
    let mut context_lock = context.lock().await;
    if let Some(ctx) = context_lock.as_mut() {
        if ctx.state != UploadState::WorkersOpening { return; }

        let num_workers = calculate_workers(ctx.metadata.file_size);
        log::info!("> Worker request approved. Opening {} stream(s)...", num_workers);

        for i in 0..num_workers {
            let conn_clone = connection.clone();
            tokio::spawn(async move {
                match conn_clone.open_bi().await {
                    Ok((_send, _recv)) => log::info!("  - Worker stream {} opened successfully.", i + 1),
                    Err(e) => log::error!("! Failed to open worker stream {}: {}", i + 1, e),
                }
            });
        }
        ctx.state = UploadState::Streaming;
    }
}