/* src/console/app.rs */

use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize};
use std::sync::Arc;
use tui_logger::TuiWidgetState;

#[derive(Clone, Default)]
pub struct Stats {
    pub tx_bytes: Arc<AtomicU64>,
    pub rx_bytes: Arc<AtomicU64>,
    pub last_msg_id: Arc<AtomicU8>,
    pub pool_count: Arc<AtomicUsize>,
}

pub struct App {
    pub input: String,
    pub status: String,
    pub should_quit: bool,
    pub info_log_state: TuiWidgetState,
    pub debug_log_state: TuiWidgetState,
    pub stats: Stats,
}

impl App {
    pub fn new() -> Self {
        App {
            input: String::new(),
            status: "Press 'Ctrl-C' to quit.".to_string(),
            should_quit: false,
            info_log_state: TuiWidgetState::new(),
            debug_log_state: TuiWidgetState::new(),
            stats: Stats::default(),
        }
    }
}