/* src/wsm/endpoints.rs */

use crate::console::app::Stats;
use crate::quic::{auth, keepalive};
use crate::setup::config::{Config, RfsConfig};
use crate::wsm::header::{PayloadType, WsmHeader, OPCODE_ERROR_FATAL, RESERVED_FINAL_FLAG};
use crate::wsm::msg_id;
use quinn::RecvStream;
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    Unauthenticated,
    Authenticated,
}

pub type InFlightPings = Arc<Mutex<HashMap<u8, Instant>>>;

// Server-side dispatcher
pub async fn dispatch_server(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
    auth_state: Arc<Mutex<AuthState>>,
    cfg: &Config,
) -> ControlFlow<()> {
    let state = *auth_state.lock().await;
    if state == AuthState::Unauthenticated && !matches!(header.opcode, 0x01 | 0x03) {
        log::warn!(
            "! WSM-Server: Denying access to opcode {:#04X} for unauthenticated client.",
            header.opcode
        );
        auth::send_unauthorized_response(header.message_id, tx).await;
        return ControlFlow::Break(());
    }

    match header.opcode {
        0x01 => {
            keepalive::handle_ping_request(header.message_id, tx, cfg).await;
        }
        0x03 => {
            if !auth::handle_auth_request(header, recv, tx, auth_state, cfg).await {
                return ControlFlow::Break(());
            }
        }
        0x05 => {
            handle_rfs_list_request(header.message_id, tx, cfg).await;
        }
        _ => {
            log::warn!(
                "! WSM-Server: Received unknown opcode: {:#04X}",
                header.opcode
            );
        }
    }
    ControlFlow::Continue(())
}

// Client-side dispatcher
pub async fn dispatch_client(
    header: &WsmHeader,
    recv: &mut RecvStream,
    in_flight_pings: InFlightPings,
    auth_state: Arc<Mutex<AuthState>>,
    stop_reconnecting: Arc<AtomicBool>,
    cfg: &Config,
    stats: Stats,
) -> ControlFlow<()> {
    stats
        .rx_bytes
        .fetch_add(header.payload_len as u64, Ordering::Relaxed);
    match header.opcode {
        0x00 => {
            if !auth::handle_auth_response(header, recv, auth_state, stop_reconnecting).await {
                return ControlFlow::Break(());
            }
        }
        0x02 => {
            keepalive::handle_pong_response(header.message_id, in_flight_pings, cfg).await;
        }
        0x04 => {
            handle_rfs_list_response(header, recv).await;
        }
        OPCODE_ERROR_FATAL => {
            log::error!("! WSM-Client: Received fatal error from server.");
            if header.payload_len > 0 {
                let mut reason_buf = vec![0; header.payload_len as usize];
                if recv.read_exact(&mut reason_buf).await.is_ok() {
                    log::error!("! Server reason: {}", String::from_utf8_lossy(&reason_buf));
                }
            }
            stop_reconnecting.store(true, Ordering::SeqCst);
            return ControlFlow::Break(());
        }
        _ => {
            log::warn!(
                "! WSM-Client: Received unknown opcode: {:#04X}",
                header.opcode
            );
        }
    }

    // If message is final, remove its ID from the in-use pool
    if header.is_final() {
        msg_id::remove_msg_id(header.message_id).await;
    }

    ControlFlow::Continue(())
}

// Server handler for rfs list request (0x05)
async fn handle_rfs_list_request(message_id: u8, tx: mpsc::Sender<Vec<u8>>, cfg: &Config) {
    let rfs_list = cfg.rfs.as_ref().unwrap();

    match serde_json::to_string(rfs_list) {
        Ok(json_payload) => {
            let payload_bytes = json_payload.as_bytes();
            let response_header = WsmHeader::with_reserved(
                0x04, // Response opcode
                message_id,
                PayloadType::Json,
                payload_bytes.len() as u32,
                RESERVED_FINAL_FLAG,
            );

            let mut response = response_header.to_bytes().to_vec();
            response.extend_from_slice(payload_bytes);

            if tx.send(response).await.is_err() {
                log::error!("! WSM-Server: Failed to send rfs list response to channel.");
            }
        }
        Err(e) => {
            log::error!("! WSM-Server: Failed to serialize rfs list: {}", e);
        }
    }
}

// Client handler for rfs list response (0x04)
async fn handle_rfs_list_response(header: &WsmHeader, recv: &mut RecvStream) {
    if header.payload_len == 0 {
        log::info!("> Received empty volume list from server.");
        return;
    }

    let mut payload_buf = vec![0; header.payload_len as usize];
    if recv.read_exact(&mut payload_buf).await.is_err() {
        log::error!("! WSM-Client: Failed to read rfs list payload.");
        return;
    }

    // MODIFIED: Directly use the shared RfsConfig struct for deserialization.
    // This avoids defining a local struct inside an async function, which is cleaner
    // and prevents potential bugs related to the compiler-generated state machine.
    match serde_json::from_slice::<Vec<RfsConfig>>(&payload_buf) {
        Ok(rfs_list) => {
            let mut display_text = String::from("Volume List Received:\n");
            for (i, rfs) in rfs_list.iter().enumerate() {
                display_text.push_str(&format!(
                    " [{}] dev_name: '{}', bind_path: '{}'\n",
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