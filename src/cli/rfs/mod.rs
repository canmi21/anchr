/* src/cli/rfs/mod.rs */

mod list;

use log::info;
use tokio::sync::mpsc;

pub async fn handle_command(args: Vec<&str>, tx: mpsc::Sender<Vec<u8>>) {
    match args.first() {
        Some(&"list") => {
            let sub_args = args.get(1..).unwrap_or(&[]).to_vec();
            list::execute(sub_args, tx).await;
        }
        _ => {
            info!("Unknown rfs command. Available commands: list");
        }
    }
}