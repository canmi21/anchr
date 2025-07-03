/* src/console/cli.rs */

use crate::{
    cli as command_cli,
    console::{app::App, ui},
    quic::client::run_network_tasks,
    rfs,
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
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time;
use tui_logger::{init_logger, set_default_level, set_level_for_target};

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

    // Create the shared context for the entire client session.
    let shared_context: rfs::SharedUploadContext = Arc::new(Mutex::new(None));

    // Stats updater task
    tokio::spawn(async move {
        loop {
            let count = msg_id::get_pool_size().await;
            stats_for_updater
                .pool_count
                .store(count, Ordering::Relaxed);
            time::sleep(Duration::from_secs(1)).await;
        }
    });

    // Network task now gets the context.
    let network_tx = tx.clone();
    let network_context = shared_context.clone();
    tokio::spawn(async move {
        run_network_tasks(cfg, stats_for_network, network_tx, rx, network_context).await;
    });

    // Main UI loop
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
                        let command_context = shared_context.clone(); // Clone context for the command.
                        app.input.clear();

                        tokio::spawn(async move {
                            // Pass the context to the command dispatcher.
                            command_cli::dispatch_command(
                                &input_to_process,
                                command_tx,
                                command_context,
                            )
                            .await;
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