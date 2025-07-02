/* src/cli/mod.rs */

mod drop;
mod ping;

use log::info;
use tokio::sync::mpsc;

pub async fn dispatch_command(input: &str, tx: mpsc::Sender<Vec<u8>>) {
    let mut parts = input.trim().split_whitespace();
    if let Some(command) = parts.next() {
        let args: Vec<&str> = parts.collect();
        info!("Executing command: '{}' with args: {:?}", command, args);

        match command.to_lowercase().as_str() {
            "ping" => {
                ping::handle_command(args, tx).await;
            }
            "drop" => {
                drop::handle_command(args).await;
            }
            _ => {
                info!("Unknown command: {}", command);
            }
        }
    }
}