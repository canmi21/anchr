/* src/wsm/endpoints.rs */

use crate::console::app::Stats;
use crate::quic::{auth, keepalive};
use crate::setup::config::Config;
use crate::wsm::header::{WsmHeader, OPCODE_ERROR_FATAL};
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

pub async fn dispatch_server(
    header: &WsmHeader,
    recv: &mut RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
    auth_state: Arc<Mutex<AuthState>>,
    cfg: &Config,
) -> ControlFlow<()> {
    let state = *auth_state.lock().await;
    if state == AuthState::Unauthenticated && !matches!(header.opcode, 0x01 | 0x03) {
        log::warn!("! WSM-Server: Denying access to opcode {:#04X} for unauthenticated client.", header.opcode);
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
        _ => {
            log::warn!("! WSM-Server: Received unknown opcode: {:#04X}", header.opcode);
        }
    }
    ControlFlow::Continue(())
}

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
            log::warn!("! WSM-Client: Received unknown opcode: {:#04X}", header.opcode);
        }
    }
    ControlFlow::Continue(())
}