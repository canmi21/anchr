/* src/cli/rfs/upload.rs */

use crate::rfs::{SharedUploadContext, UploadContext, UploadMetadata, UploadState};
use crate::wsm::header::{PayloadType, WsmHeader};
use crate::wsm::msg_id;
use log::{error, info, warn};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path};
use tokio::fs as tokio_fs;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

pub async fn execute(
    args: Vec<&str>,
    tx: mpsc::Sender<Vec<u8>>,
    context: SharedUploadContext,
) {
    if args.len() != 2 {
        error!("Usage: rfs upload <target_dir> <local_path_to_file>");
        return;
    }

    if context.lock().await.is_some() {
        error!("Another upload is already in progress. Please wait for it to complete.");
        return;
    }

    let target_dir = args[0].to_string();
    let local_path_str = args[1];
    let local_path = Path::new(local_path_str);

    let metadata = match fs::metadata(local_path) {
        Ok(meta) => {
            if !meta.is_file() {
                error!("'{}' is not a file.", local_path_str);
                return;
            }
            meta
        }
        Err(e) => {
            error!("Failed to access file '{}': {}", local_path_str, e);
            return;
        }
    };

    let file_name = match local_path.file_name() {
        Some(name) => name.to_string_lossy().to_string(),
        None => {
            error!("Could not determine filename from path '{}'.", local_path_str);
            return;
        }
    };

    let re = Regex::new(r"^[a-zA-Z0-9_.@-]+$").unwrap();
    if !re.is_match(&file_name) {
        error!("Filename '{}' contains invalid characters.", file_name);
        warn!("Allowed characters are: a-z, A-Z, 0-9, _, ., -, @");
        return;
    }

    // --- MODIFIED: Calculate hash asynchronously to prevent blocking ---
    let file_hash = match calculate_hash_async(local_path).await {
        Ok(hash) => hash,
        Err(e) => {
            error!("Failed to read and hash file '{}': {}", local_path_str, e);
            return;
        }
    };

    let file_size = metadata.len();
    let upload_meta = UploadMetadata {
        target_dir,
        file_name: file_name.clone(),
        file_size,
        file_hash,
    };

    let json_payload = serde_json::to_string(&upload_meta).unwrap();

    if let Some(msg_id) = msg_id::create_new_msg_id().await {
        let mut ctx_lock = context.lock().await;

        *ctx_lock = Some(UploadContext {
            metadata: upload_meta.clone(),
            local_file_path: local_path.to_path_buf(),
            message_id: msg_id,
            state: UploadState::Initiated,
            chunk_queue: Default::default(),
            total_chunks: 0,
            completed_chunks: Default::default(),
        });

        let header = WsmHeader::new(
            0x06,
            msg_id,
            PayloadType::Json,
            json_payload.len() as u32,
        );

        let mut message = header.to_bytes().to_vec();
        message.extend_from_slice(json_payload.as_bytes());

        info!(
            "Initiating upload for '{}' ({} bytes)...",
            upload_meta.file_name, upload_meta.file_size
        );
        if tx.send(message).await.is_err() {
            error!("Failed to send upload initiation command.");
            *ctx_lock = None;
            msg_id::remove_msg_id(msg_id).await;
        } else {
            info!(
                "Upload initiation for '{}' sent (id: {}). Waiting for server ACK...",
                upload_meta.file_name, msg_id
            );
        }
    } else {
        error!("Failed to initiate upload: message ID pool is full.");
    }
}

/// A helper function to calculate file hash asynchronously.
async fn calculate_hash_async(file_path: &Path) -> std::io::Result<String> {
    let mut file = tokio_fs::File::open(file_path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192]; // 8KB buffer

    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}