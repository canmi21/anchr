/* src/quic/auth.rs */

use crate::setup::config::Config;
use crate::wsm::endpoints::AuthState;
use crate::wsm::header::{PayloadType, WsmHeader, RESERVED_FINAL_FLAG, OPCODE_ERROR_FATAL};
use log::info;
use quinn::RecvStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

pub async fn handle_auth_request(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
    auth_state: Arc<Mutex<AuthState>>,
    cfg: &Config,
) -> bool {
    let mut token_buf = vec![0; header.payload_len as usize];
    if recv.read_exact(&mut token_buf).await.is_err() {
        return false;
    }

    let received_token = String::from_utf8_lossy(&token_buf);
    let expected_token = &cfg.setup.auth_token;

    if received_token == *expected_token {
        println!("  -> WSM: Client authenticated successfully.");
        let mut state = auth_state.lock().await;
        *state = AuthState::Authenticated;
        let response_header = WsmHeader::with_reserved(
            0x00,
            header.message_id,
            PayloadType::Raw,
            0,
            RESERVED_FINAL_FLAG,
        );
        let _ = tx.send(response_header.to_bytes().to_vec()).await;
        true
    } else {
        println!("  -> WSM: Client authentication failed (token mismatch).");
        let reason = "Invalid authentication token".as_bytes();
        let response_header = WsmHeader::with_reserved(
            0x00,
            header.message_id,
            PayloadType::Raw,
            reason.len() as u32,
            RESERVED_FINAL_FLAG,
        );
        let mut response = response_header.to_bytes().to_vec();
        response.extend_from_slice(reason);
        let _ = tx.send(response).await;
        false
    }
}

pub async fn handle_auth_response(
    header: &WsmHeader,
    recv: &mut RecvStream,
    auth_state: Arc<Mutex<AuthState>>,
    stop_reconnecting: Arc<AtomicBool>,
) -> bool {
    if header.is_final() {
        if header.payload_len == 0 {
            // FIX: Use info! macro to log to the client's TUI
            info!("WSM: Authentication successful.");
            let mut state = auth_state.lock().await;
            *state = AuthState::Authenticated;
            return true;
        } else {
            let mut reason_buf = vec![0; header.payload_len as usize];
            if recv.read_exact(&mut reason_buf).await.is_ok() {
                let reason = String::from_utf8_lossy(&reason_buf);
                // This is also a client-side print, so it should be a log macro
                info!("! WSM: Authentication failed. Reason: {}", reason);
            } else {
                info!("! WSM: Authentication failed. Could not read reason.");
            }
            stop_reconnecting.store(true, Ordering::SeqCst);
            return false;
        }
    }
    true
}

pub async fn send_unauthorized_response(msg_id: u8, tx: mpsc::Sender<Vec<u8>>) {
    let reason = "Unauthenticated".as_bytes();
    let header = WsmHeader::with_reserved(
        OPCODE_ERROR_FATAL,
        msg_id,
        PayloadType::Raw,
        reason.len() as u32,
        RESERVED_FINAL_FLAG,
    );
    let mut response = header.to_bytes().to_vec();
    response.extend_from_slice(reason);
    let _ = tx.send(response).await;
}