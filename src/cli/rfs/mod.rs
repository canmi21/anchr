/* src/cli/rfs/mod.rs */

mod list;
mod upload;

use crate::rfs::SharedUploadContext;
use log::info;
use tokio::sync::mpsc;

pub async fn handle_command(
    args: Vec<&str>,
    tx: mpsc::Sender<Vec<u8>>,
    context: SharedUploadContext,
) {
    match args.first() {
        Some(&"list") => {
            let sub_args = args.get(1..).unwrap_or(&[]).to_vec();
            list::execute(sub_args, tx).await;
        }
        Some(&"upload") => {
            let sub_args = args.get(1..).unwrap_or(&[]).to_vec();
            upload::execute(sub_args, tx, context).await;
        }
        _ => {
            info!("Unknown rfs command. Available commands: list, upload");
        }
    }
}