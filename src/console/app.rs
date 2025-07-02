/* src/console/app.rs */

use tui_logger::TuiWidgetState;

pub struct App {
    pub input: String,
    pub status: String,
    pub should_quit: bool,
    pub info_log_state: TuiWidgetState,
    pub debug_log_state: TuiWidgetState,
}

impl App {
    pub fn new() -> Self {
        App {
            input: String::new(),
            status: "Press 'Ctrl-C' to quit.".to_string(),
            should_quit: false,
            info_log_state: TuiWidgetState::new(),
            debug_log_state: TuiWidgetState::new(),
        }
    }
}