/* src/rfs/list.rs */

use crate::setup::config::{Config, RfsConfig};
use crate::wsm::header::{PayloadType, WsmHeader, RESERVED_FINAL_FLAG};
use quinn::RecvStream;
use tokio::sync::mpsc;

// [SERVER-SIDE] Handles the `rfs list` (0x05) request.
pub async fn handle_request(message_id: u8, tx: mpsc::Sender<Vec<u8>>, cfg: &Config) {
    let rfs_list = cfg.rfs.as_ref().unwrap();
    match serde_json::to_string(rfs_list) {
        Ok(json_payload) => {
            let payload_bytes = json_payload.as_bytes();
            let response_header = WsmHeader::with_reserved(
                0x04, // Opcode for list response
                message_id,
                PayloadType::Json,
                payload_bytes.len() as u32,
                RESERVED_FINAL_FLAG,
            );
            let mut response = response_header.to_bytes().to_vec();
            response.extend_from_slice(payload_bytes);
            if tx.send(response).await.is_err() {
                eprintln!("! WSM-Server: Failed to send rfs list response to channel.");
            }
        }
        Err(e) => {
            eprintln!("! WSM-Server: Failed to serialize rfs list: {}", e);
        }
    }
}

// [CLIENT-SIDE] Handles the `rfs list` (0x04) response.
pub async fn handle_response(header: &WsmHeader, recv: &mut RecvStream) {
    if header.payload_len == 0 {
        log::info!("> Received empty volume list from server.");
        return;
    }
    let mut payload_buf = vec![0; header.payload_len as usize];
    if recv.read_exact(&mut payload_buf).await.is_err() {
        log::error!("! WSM-Client: Failed to read rfs list payload.");
        return;
    }
    match serde_json::from_slice::<Vec<RfsConfig>>(&payload_buf) {
        Ok(rfs_list) => {
            let mut display_text = String::from("Volume List Received:\n");
            for (i, rfs) in rfs_list.iter().enumerate() {
                display_text.push_str(&format!(
                    "  [{}] dev_name: '{}', bind_path: '{}'\n",
                    i, rfs.dev_name, rfs.bind_path
                ));
            }
            log::info!("{}", display_text.trim_end());
        }
        Err(e) => {
            log::error!("! WSM-Client: Failed to deserialize rfs list: {}", e);
        }
    }
}