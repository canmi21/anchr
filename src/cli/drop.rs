/* src/cli/drop.rs */

use crate::wsm::msg_id;
use log::info;

pub async fn handle_command(_args: Vec<&str>) {
    let in_use_count = msg_id::get_pool_size().await;

    if in_use_count == 0 {
        info!("Command 'drop': Message ID pool is already empty.");
    } else {
        msg_id::clear_msg_id_pool().await;
        info!(
            "Command 'drop': Successfully cleared all {} in-use message IDs.",
            in_use_count
        );
    }
}