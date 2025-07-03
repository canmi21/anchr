/* src/rfs/mod.rs */

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod list;
pub mod upload;

// ... (UploadState and UploadContext structs are unchanged) ...
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UploadMetadata {
    pub target_dir: String,
    pub file_name: String,
    pub file_size: u64,
    pub file_hash: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadState {
    Initiated,
    WorkersOpening,
    Streaming,
}
#[derive(Debug, Clone)]
pub struct UploadContext {
    pub metadata: UploadMetadata,
    pub message_id: u8,
    pub state: UploadState,
}

pub type SharedUploadContext = Arc<Mutex<Option<UploadContext>>>;

// Add this new enum for the preparation result
#[derive(Debug)]
pub enum PreparationResult {
    New,
    Resumable,
}