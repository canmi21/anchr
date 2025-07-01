/* src/quic/keepalive.rs */

use crate::setup::config::Config;
use crate::wsm::header::{PayloadType, WsmHeader, RESERVED_FINAL_FLAG};
use crate::wsm::msg_id;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex};

pub type InFlightPings = Arc<Mutex<HashMap<u8, Instant>>>;

pub fn build_ping_header(message_id: u8) -> [u8; 8] {
    WsmHeader::new(0x01, message_id, PayloadType::Raw, 0).to_bytes()
}

pub fn build_pong_header(message_id: u8) -> [u8; 8] {
    WsmHeader::with_reserved(0x02, message_id, PayloadType::Raw, 0, RESERVED_FINAL_FLAG).to_bytes()
}

pub async fn build_client_ping() -> Option<[u8; 8]> {
    msg_id::create_new_msg_id().await.map(build_ping_header)
}

// (SERVER) Handles a received PING message by sending a PONG back.
pub async fn handle_ping_request(msg_id: u8, tx: mpsc::Sender<Vec<u8>>, cfg: &Config) {
    if cfg.setup.log_level == "debug" {
        println!("  -> WSM: Handling PING with msg_id: {}. Responding with PONG.", msg_id);
    }
    let pong_header = build_pong_header(msg_id);
    if let Err(e) = tx.send(pong_header.to_vec()).await {
        println!("! WSM: Failed to queue PONG response: {}", e);
    }
}

// (CLIENT) Handles a received PONG message by removing the msg_id from the in-flight map.
pub async fn handle_pong_response(msg_id: u8, in_flight_pings: InFlightPings, cfg: &Config) {
    if cfg.setup.log_level == "debug" {
        println!("  -> WSM: Handling PONG for msg_id: {}", msg_id);
    }
    if in_flight_pings.lock().await.remove(&msg_id).is_some() {
        if msg_id::remove_msg_id(msg_id).await {
            if cfg.setup.log_level == "debug" {
                println!("  -- Correctly cleared msg_id {} from pool.", msg_id);
            }
        }
    } else {
        println!("  -- Warning: Received PONG for an untracked or timed-out msg_id: {}", msg_id);
    }
}