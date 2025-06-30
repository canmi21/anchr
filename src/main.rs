mod setup;
mod quic;

use setup::gen_conf::generate_default_config;
use setup::cert::generate_certificate;
use setup::config::Config;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        generate_default_config("anchr.toml");
        generate_certificate("cert.crt", "cert.key");
        println!("> Default config and certificate generated");
        return;
    }

    if args.len() == 3 && args[1] == "-c" {
        let config = Config::from_file(&args[2]);
        println!("> Mode: {}", config.setup.mode);
        println!("> Listening on {}:{}",
            config.network.listen, config.network.port);
        return;
    }

    println!("! Invalid usage");
}
