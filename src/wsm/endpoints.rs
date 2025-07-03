/* src/wsm/endpoints.rs */

use crate::console::app::Stats;
use crate::quic::{auth, keepalive};
use crate::rfs::{self, SharedUploadContext, UploadState};
use crate::setup::config::Config;
use crate::wsm::header::{WsmHeader, OPCODE_ERROR_FATAL};
use crate::wsm::msg_id;
use quinn::{Connection, RecvStream};
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
        eprintln!("! WSM-Server: Denying opcode {:#04X} for unauthenticated client.", header.opcode);
        auth::send_unauthorized_response(header.message_id, tx).await;
        return ControlFlow::Break(());
    }

    match header.opcode {
        0x01 => keepalive::handle_ping_request(header.message_id, tx, cfg).await,
        0x03 => {
            if !auth::handle_auth_request(header, recv, tx, auth_state, cfg).await {
                return ControlFlow::Break(());
            }
        }
        // Delegate RFS logic to the rfs module
        0x05 => rfs::list::handle_request(header.message_id, tx, cfg).await,
        0x06 => rfs::upload::handle_init_request(header, recv, tx, cfg).await,
        0x07 => rfs::upload::handle_worker_request(header, recv, tx).await,
        _ => {
            eprintln!("! WSM-Server: Received unknown opcode: {:#04X}", header.opcode);
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
    context: SharedUploadContext,
    tx: mpsc::Sender<Vec<u8>>,
    connection: Arc<Connection>,
) -> ControlFlow<()> {
    stats.rx_bytes.fetch_add(header.payload_len as u64, Ordering::Relaxed);

    match header.opcode {
        0x00 => { // Generic ACK/Reply
            let context_lock = context.lock().await;
            if let Some(ctx) = context_lock.as_ref() {
                if ctx.message_id == header.message_id {
                    let state = ctx.state; // Copy state to avoid borrowing issues
                    drop(context_lock); // Release lock before async calls
                    match state {
                        UploadState::Initiated => {
                            let mut payload_buf = [0; 1];
                            if header.payload_len == 1 && recv.read_exact(&mut payload_buf).await.is_ok() {
                                match payload_buf[0] {
                                    1 => log::info!("> Server acknowledged NEW upload request."),
                                    2 => log::info!("> Server acknowledged RESUMABLE upload."),
                                    _ => log::warn!("> Server sent unknown ACK code."),
                                }
                                rfs::upload::handle_init_ack(context, tx).await;
                            } else {
                                log::error!("> Server sent invalid ACK for upload initiation.");
                            }
                        }
                        UploadState::WorkersOpening => {
                            rfs::upload::handle_worker_ack(context, connection).await;
                        }
                        _ => {}
                    }
                    msg_id::remove_msg_id(header.message_id).await;
                    return ControlFlow::Continue(());
                }
            }
            drop(context_lock);
            if *auth_state.lock().await == AuthState::Unauthenticated {
                if !auth::handle_auth_response(header, recv, auth_state, stop_reconnecting).await {
                    return ControlFlow::Break(());
                }
            }
        }
        0x02 => keepalive::handle_pong_response(header.message_id, in_flight_pings, cfg).await,
        0x04 => rfs::list::handle_response(header, recv).await,
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

    if header.is_final() {
        msg_id::remove_msg_id(header.message_id).await;
    }

    ControlFlow::Continue(())
}