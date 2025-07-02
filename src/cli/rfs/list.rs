/* src/cli/rfs/list.rs */

use crate::wsm::header::{PayloadType, WsmHeader};
use crate::wsm::msg_id;
use log::{error, info};
use tokio::sync::mpsc;

pub async fn execute(_args: Vec<&str>, tx: mpsc::Sender<Vec<u8>>) {
    info!("Requesting volume list from server...");

    if let Some(msg_id) = msg_id::create_new_msg_id().await {
        // Create a full 8-byte WsmHeader, with 0 payload length
        let header = WsmHeader::new(
            0x05, // rfs list opcode
            msg_id,
            PayloadType::Raw,
            0,
        );
        let message = header.to_bytes().to_vec();

        if let Err(e) = tx.send(message).await {
            error!("Failed to send 'rfs list' command: {}", e);
            // If sending fails, release the ID
            msg_id::remove_msg_id(msg_id).await;
        } else {
            info!("'rfs list' command sent (id: {}). Waiting for response...", msg_id);
        }
    } else {
        error!("Failed to create 'rfs list' command: message ID pool is full.");
    }
}