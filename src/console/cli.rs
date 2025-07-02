/* src/console/cli.rs */

use crate::{
    console::{app::App, ui},
    quic::client::run_network_tasks,
    setup::config::Config,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::LevelFilter;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tui_logger::{init_logger, set_default_level};

pub async fn run_tui_client(cfg: Config) -> io::Result<()> {
    init_logger(LevelFilter::Trace).unwrap();
    set_default_level(LevelFilter::Trace);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    // FIX: Reassign the returned state back to the app fields.
    app.info_log_state = app.info_log_state.set_default_display_level(LevelFilter::Info);
    app.debug_log_state = app.debug_log_state.set_default_display_level(LevelFilter::Debug);

    tokio::spawn(async move {
        run_network_tasks(cfg).await;
    });

    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Enter => {
                        // TODO: Handle input submission
                        app.input.clear();
                    }
                    KeyCode::Char(c) => {
                        app.input.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}