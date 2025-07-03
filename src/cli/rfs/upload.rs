/* src/cli/rfs/upload.rs */

use crate::rfs::{SharedUploadContext, UploadContext, UploadMetadata, UploadState};
use crate::wsm::header::{PayloadType, WsmHeader};
use crate::wsm::msg_id;
use log::{error, info, warn};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
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

    // Check if another upload is already in progress
    if context.lock().await.is_some() {
        error!("Another upload is already in progress. Please wait for it to complete.");
        return;
    }

    let target_dir = args[0].to_string();
    let local_path_str = args[1];
    let local_path = Path::new(local_path_str);

    // ... (File validation logic is the same)
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
    let re = Regex::new(r"^[a-zA-Z0-9_.-@]+$").unwrap();
    if !re.is_match(&file_name) {
        error!("Filename '{}' contains invalid characters.", file_name);
        warn!("Allowed characters are: a-z, A-Z, 0-9, _, ., -, @");
        return;
    }
    let file_content = match fs::read(local_path) {
        Ok(content) => content,
        Err(e) => {
            error!("Failed to read file content '{}': {}", local_path_str, e);
            return;
        }
    };
    let mut hasher = Sha256::new();
    hasher.update(&file_content);
    let hash_bytes = hasher.finalize();
    let file_hash = format!("{:x}", hash_bytes);
    let file_size = metadata.len();

    let upload_meta = UploadMetadata {
        target_dir,
        file_name: file_name.clone(),
        file_size,
        file_hash,
    };

    let json_payload = serde_json::to_string(&upload_meta).unwrap();

    if let Some(msg_id) = msg_id::create_new_msg_id().await {
        // Create and save the context BEFORE sending the message
        let mut ctx_lock = context.lock().await;
        *ctx_lock = Some(UploadContext {
            metadata: upload_meta.clone(),
            message_id: msg_id,
            state: UploadState::Initiated,
        });

        let header = WsmHeader::new(
            0x06, // Upload opcode
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
            // Clear context on failure
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