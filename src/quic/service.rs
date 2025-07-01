/* src/quic/service.rs */

use crate::setup::config::Config;
use crate::wsm::endpoints::{self, AuthState};
use crate::wsm::header::WsmHeader;
use quinn::Connection;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time;

pub async fn handle_connection(conn: Connection, cfg: Config) {
    println!("-> Handing connection from {} to service.", conn.remote_address());
    let auth_state = Arc::new(Mutex::new(AuthState::Unauthenticated));

    match conn.accept_bi().await {
        Ok((mut control_send, mut control_recv)) => {
            println!("  -- Control stream {} established.", control_send.id());
            let (tx, mut rx) = mpsc::channel::<Vec<u8>>(32);

            tokio::spawn(async move {
                while let Some(msg_bytes) = rx.recv().await {
                    if let Err(e) = control_send.write_all(&msg_bytes).await {
                        println!("! Failed to send message on control stream: {}", e);
                        break;
                    }
                }
                println!("  -- Sender task for stream {} finished.", control_send.id());
            });

            let mut header_buf = [0u8; 8];
            loop {
                let timeout_result = time::timeout(
                    Duration::from_millis(1500),
                    control_recv.read_exact(&mut header_buf),
                )
                .await;

                match timeout_result {
                    Ok(Ok(())) => {
                        let header = WsmHeader::from_bytes(&header_buf);
                        if let ControlFlow::Break(_) = endpoints::dispatch_server(
                            &header,
                            &mut control_recv,
                            tx.clone(),
                            auth_state.clone(),
                            &cfg,
                        )
                        .await
                        {
                            println!("! Dispatcher requested connection termination.");
                            conn.close(2u32.into(), b"auth failure");
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        println!("! Read error on control stream: {}. Closing.", e);
                        break;
                    }
                    Err(_) => {
                        println!("! Watchdog timeout: No PING received in 1.5s. Closing connection.");
                        conn.close(0u32.into(), b"keep-alive timeout");
                        break;
                    }
                }
            }
        }
        Err(e) => {
            println!("! Failed to accept the initial control stream: {}", e);
        }
    }
    println!("- Connection from {} closed.", conn.remote_address());
}