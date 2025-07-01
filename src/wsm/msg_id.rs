/* src/wsm/msg_id.rs */

use lazy_static::lazy_static;
use std::collections::HashSet;
use tokio::sync::Mutex; // Use tokio's Mutex

lazy_static! {
    static ref MSG_ID_POOL: Mutex<HashSet<u8>> = Mutex::new(HashSet::new());
}

pub async fn create_new_msg_id() -> Option<u8> {
    let mut pool = MSG_ID_POOL.lock().await;
    if pool.len() == (u8::MAX as usize + 1) {
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
        println!("- Cleared message ID pool due to extended disconnection.");
    }
}