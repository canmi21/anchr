/* src/rfs/mod.rs */

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod list;
pub mod upload;
pub mod worker;
pub mod verify;

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
    Finishing,
}

#[derive(Debug, Clone)]
pub struct UploadContext {
    pub metadata: UploadMetadata,
    pub local_file_path: PathBuf,
    pub message_id: u8,
    pub state: UploadState,
    pub chunk_queue: Arc<Mutex<VecDeque<u64>>>,
    pub total_chunks: u64,
    pub completed_chunks: Arc<AtomicU64>,
}

pub type SharedUploadContext = Arc<Mutex<Option<UploadContext>>>;

#[derive(Debug)]
pub enum PreparationResult {
    New,
    Resumable,
}