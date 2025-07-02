/* src/cli/drop.rs */

use crate::wsm::msg_id;
use log::info;

pub async fn handle_command(_args: Vec<&str>) {
    let dropped_ids = msg_id::drain_msg_id_pool().await;

    if dropped_ids.is_empty() {
        info!("Command 'drop': Message ID pool is already empty.");
    } else {
        info!(
            "Command 'drop': Cleared {} message IDs from the pool.",
            dropped_ids.len()
        );

        let display_count = std::cmp::min(dropped_ids.len(), 10);
        let ids_to_display: String = dropped_ids[..display_count]
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<String>>()
            .join(", ");

        if dropped_ids.len() > 10 {
            info!("Dropped IDs (first 10): [{:?}...]", ids_to_display);
        } else {
            info!("Dropped IDs: [{:?}]", ids_to_display);
        }
    }
}