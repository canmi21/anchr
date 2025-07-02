/* src/console/cli.rs */

use crate::{
    cli as command_cli,
    console::{app::App, ui},
    quic::client::run_network_tasks,
    setup::config::Config,
    wsm::msg_id,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::LevelFilter;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;
use tui_logger::{init_logger, set_default_level, set_level_for_target}; // Import added

pub async fn run_tui_client(cfg: Config) -> io::Result<()> {
    init_logger(LevelFilter::Trace).unwrap();
    set_default_level(LevelFilter::Trace);
    set_level_for_target("quinn::connection", LevelFilter::Info);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.info_log_state = app.info_log_state.set_default_display_level(LevelFilter::Info);
    app.debug_log_state = app.debug_log_state.set_default_display_level(LevelFilter::Debug);

    let stats_for_network = app.stats.clone();
    let stats_for_updater = app.stats.clone();

    let (tx, rx) = mpsc::channel::<Vec<u8>>(32);

    tokio::spawn(async move {
        loop {
            let count = msg_id::get_pool_size().await;
            stats_for_updater
                .pool_count
                .store(count, Ordering::Relaxed);
            time::sleep(Duration::from_secs(1)).await;
        }
    });

    let network_tx = tx.clone();
    tokio::spawn(async move {
        run_network_tasks(cfg, stats_for_network, network_tx, rx).await;
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
                        let command_tx = tx.clone();
                        let input_to_process = app.input.clone();
                        app.input.clear();

                        tokio::spawn(async move {
                            command_cli::dispatch_command(&input_to_process, command_tx).await;
                        });
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