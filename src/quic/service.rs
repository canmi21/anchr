/* src/quic/service.rs */

use crate::quic::keepalive;
use quinn::Connection;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// Server will drop the client if no keepalive is received within this duration.
const CLIENT_TIMEOUT_MS: u64 = 1500;

pub async fn handle_authenticated_client(connection: Connection) {
    println!("+ Starting service handler for {}", connection.remote_address());

    let last_ping = Arc::new(Mutex::new(Instant::now()));
    let last_ping_clone = last_ping.clone();
    let connection_clone = connection.clone();

    // Spawn a background task to monitor for client timeouts.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await; // Check every 500ms
            if last_ping_clone.lock().unwrap().elapsed() > Duration::from_millis(CLIENT_TIMEOUT_MS) {
                println!(
                    "! Client {} timed out. Closing connection.",
                    connection_clone.remote_address()
                );
                connection_clone.close(2u32.into(), b"keepalive timeout");
                break;
            }
            // If connection is already closed by other means, stop this task.
            if connection_clone.close_reason().is_some() {
                break;
            }
        }
    });

    // Main loop to accept streams from the client.
    loop {
        match connection.accept_bi().await {
            Ok((mut send, mut recv)) => {
                let last_ping_update = last_ping.clone();
                tokio::spawn(async move {
                    // We read just enough data to see if it's a keepalive message.
                    let mut header_buf = vec![0; keepalive::KEEPALIVE_HEADER.len()];
                    match recv.read_exact(&mut header_buf).await {
                        Ok(_) => {
                            let message = String::from_utf8_lossy(&header_buf);
                            if message.starts_with(keepalive::KEEPALIVE_HEADER) {
                                // It's a keepalive, update the timestamp and silently send an ACK.
                                *last_ping_update.lock().unwrap() = Instant::now();
                                if send.write_all(keepalive::KEEPALIVE_ACK_HEADER.as_bytes()).await.is_ok() {
                                    let _ = send.finish();
                                }
                            } else {
                                // It's a generic message, handle it.
                                let mut full_message = header_buf;
                                if let Ok(remaining_data) = recv.read_to_end(1024 * 1024).await {
                                    full_message.extend(remaining_data);
                                }
                                handle_generic_message(send, &full_message).await;
                            }
                        }
                        Err(_) => { /* Stream closed or read error, do nothing. */ }
                    }
                });
            }
            Err(_) => {
                // Connection is closed, break the loop.
                println!(
                    "+ Connection handler for {} stopped.",
                    connection.remote_address()
                );
                break;
            }
        }
    }
}

async fn handle_generic_message(mut send: quinn::SendStream, data: &[u8]) {
    println!("+ Received generic message ({} bytes)", data.len());
    // Echo the message back.
    if send.write_all(data).await.is_ok() {
        let _ = send.finish();
    }
}
