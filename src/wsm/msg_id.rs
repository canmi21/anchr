/* src/wsm/msg_id.rs */

use lazy_static::lazy_static;
use rand; // Make sure rand is in your Cargo.toml
use std::collections::HashSet;
use tokio::sync::Mutex;

lazy_static! {
    static ref MSG_ID_POOL: Mutex<HashSet<u8>> = Mutex::new(HashSet::new());
}

pub async fn create_new_msg_id() -> Option<u8> {
    let mut pool = MSG_ID_POOL.lock().await;
    // u8::MAX is 255. The length can go from 0 to 256.
    if pool.len() >= (u8::MAX as usize) + 1 {
        return None;
    }
    loop {
        let new_id = rand::random::<u8>();
        if pool.insert(new_id) {
            return Some(new_id);
        }
    }
}

pub async fn remove_msg_id(id: u8) -> bool {
    let mut pool = MSG_ID_POOL.lock().await;
    pool.remove(&id)
}

pub async fn clear_msg_id_pool() {
    let mut pool = MSG_ID_POOL.lock().await;
    if !pool.is_empty() {
        pool.clear();
        // Use log instead of println to integrate with the TUI logger
        log::info!("- Cleared message ID pool due to new session or extended disconnection.");
    }
}

/// Returns the number of message IDs currently in use.
pub async fn get_pool_size() -> usize {
    MSG_ID_POOL.lock().await.len()
}