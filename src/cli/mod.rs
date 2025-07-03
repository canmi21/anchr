/* src/cli/mod.rs */

mod drop;
mod ping;
mod rfs;

use crate::rfs::SharedUploadContext;
use log::info;
use tokio::sync::mpsc;

pub async fn dispatch_command(
    input: &str,
    tx: mpsc::Sender<Vec<u8>>,
    context: SharedUploadContext,
) {
    let mut parts = input.trim().split_whitespace();
    if let Some(command) = parts.next() {
        let args: Vec<&str> = parts.collect();
        info!("Executing command: '{}' with args: {:?}", command, args);

        match command.to_lowercase().as_str() {
            "ping" => {
                // ping command does not need the context
                ping::handle_command(args, tx).await;
            }
            "drop" => {
                // drop command does not need the context
                drop::handle_command(args).await;
            }
            "rfs" => {
                // Only rfs commands might need the stateful context
                rfs::handle_command(args, tx, context).await;
            }
            _ => {
                info!("Unknown command: {}", command);
            }
        }
    }
}