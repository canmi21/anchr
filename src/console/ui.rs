/* src/console/ui.rs */

use crate::console::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Position},
    style::{Style, Stylize},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use tui_logger::TuiLoggerWidget;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(7),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(f.area());

    // --- WIDGET 1: Main Info Log ---
    let info_log_output = TuiLoggerWidget::default()
        .block(Block::default().title("Logs").borders(Borders::ALL))
        .output_timestamp(Some("%H:%M:%S".to_string()))
        .output_separator(' ')
        .output_file(false)
        .state(&app.info_log_state) // FIX: Pass the state object here
        .style_error(Style::default().red())
        .style_warn(Style::default().yellow())
        .style_info(Style::default().cyan());
    f.render_widget(info_log_output, chunks[0]);

    // --- WIDGET 2: Debug Panel ---
    let debug_log_output = TuiLoggerWidget::default()
        .block(Block::default().title("Debug").borders(Borders::ALL))
        .output_timestamp(Some("%H:%M:%S".to_string()))
        .output_separator(' ')
        .output_file(false)
        .state(&app.debug_log_state) // FIX: Pass the state object here
        .style_debug(Style::default().green())
        .style_trace(Style::default().magenta());
    f.render_widget(debug_log_output, chunks[1]);

    // --- Status Bar and Input Box ---
    let status_bar = Paragraph::new(app.status.as_str()).style(Style::default().gray());
    f.render_widget(status_bar, chunks[2]);

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().white())
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[3]);

    f.set_cursor_position(Position {
        x: chunks[3].x + app.input.len() as u16 + 1,
        y: chunks[3].y + 1,
    });
}