/* src/console/ui.rs */

use crate::console::app::App;
use crate::console::debug;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Position},
    style::{Style, Stylize},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use tui_logger::TuiLoggerWidget;

pub fn draw(f: &mut Frame, app: &App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(7),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(f.area());

    let info_log_output = TuiLoggerWidget::default()
        .block(Block::default().title("Logs").borders(Borders::ALL))
        .output_timestamp(Some("%H:%M:%S".to_string()))
        .output_separator(' ')
        .output_file(false)
        .state(&app.info_log_state)
        .style_error(Style::default().red())
        .style_warn(Style::default().yellow())
        .style_info(Style::default().cyan());
    f.render_widget(info_log_output, main_chunks[0]);

    let debug_log_output = TuiLoggerWidget::default()
        .block(Block::default().title("Debug").borders(Borders::ALL))
        .output_timestamp(Some("%H:%M:%S".to_string()))
        .output_separator(' ')
        .output_file(false)
        .state(&app.debug_log_state)
        .style_debug(Style::default().green())
        .style_trace(Style::default().magenta());
    f.render_widget(debug_log_output, main_chunks[1]);

    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[2]);

    let status_bar = Paragraph::new(app.status.as_str()).style(Style::default().gray());
    f.render_widget(status_bar, status_chunks[0]);

    let stats_text = debug::format_stats(&app.stats);
    let stats_bar = Paragraph::new(stats_text)
        .style(Style::default().gray())
        .alignment(Alignment::Right);
    f.render_widget(stats_bar, status_chunks[1]);

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().white())
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, main_chunks[3]);

    f.set_cursor_position(Position {
        x: main_chunks[3].x + app.input.len() as u16 + 1,
        y: main_chunks[3].y + 1,
    });
}
