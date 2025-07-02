/* src/cli/ping.rs */

use crate::quic::keepalive;
use log::{error, info};
use tokio::sync::mpsc;

pub async fn handle_command(_args: Vec<&str>, tx: mpsc::Sender<Vec<u8>>) {
    info!("Manual PING command triggered.");
    if let Some(ping_msg) = keepalive::build_client_ping().await {
        if tx.send(ping_msg.to_vec()).await.is_err() {
            error!("Failed to send manual PING: channel closed.");
        }
    } else {
        error!("Failed to create manual PING: message ID pool is full.");
    }
}