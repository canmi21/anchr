/* src/main.rs */

mod cli; // Add this line
mod console;
mod quic;
mod setup;
mod wsm;

use crate::console::cli::run_tui_client;
use setup::config::Config;
use setup::gen_conf::generate_default_config;
use std::env;

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install default CryptoProvider");

    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        generate_default_config("anchr.toml");
        println!("> Default config and certificate generated. Use '-c anchr.toml' to run.");
        return;
    }

    if args.len() == 3 && args[1] == "-c" {
        let config_path = &args[2];
        let config = Config::from_file(config_path);

        if config.setup.mode == "server" {
            quic::bootstrap::start_quic_server(config).await;
        } else if config.setup.mode == "client" {
            if let Err(e) = run_tui_client(config).await {
                eprintln!("\nApplication Error: {}\n", e);
            }
        }
        return;
    }

    println!("! Invalid usage. Use '-c <config_path>' to run or no arguments to generate a default config.");
}