/* src/setup/gen_conf.rs */

use super::cert::generate_certificate;
use pnet::datalink;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use uuid::Uuid;

// Scans for available IPv4 addresses and prompts the user to select one.
fn select_ip_address() -> String {
    // Get all non-loopback IPv4 addresses from all network interfaces.
    let ipv4_addrs: Vec<String> = datalink::interfaces()
        .into_iter()
        .flat_map(|iface| iface.ips)
        .filter(|ip| ip.is_ipv4() && !ip.ip().is_loopback())
        .map(|ip| ip.ip().to_string())
        .collect();

    match ipv4_addrs.len() {
        0 => {
            println!("> No network interfaces with a valid IPv4 address found. Falling back to 127.0.0.1.");
            "127.0.0.1".to_string()
        }
        1 => {
            let ip = ipv4_addrs.first().unwrap().clone();
            println!("> Found a single IPv4 address: {}. Using it.", ip);
            ip
        }
        _ => {
            println!("> Multiple IPv4 addresses found. Please choose one:");
            for (i, ip) in ipv4_addrs.iter().enumerate() {
                println!("  {}) {}", i + 1, ip);
            }

            // Loop until a valid selection is made.
            loop {
                print!("> Enter the number of the IP address to use: ");
                io::stdout().flush().unwrap();

                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();

                match input.trim().parse::<usize>() {
                    Ok(n) if n > 0 && n <= ipv4_addrs.len() => {
                        let selected_ip = ipv4_addrs[n - 1].clone();
                        println!("> You selected: {}", selected_ip);
                        return selected_ip;
                    }
                    _ => {
                        println!(
                            "! Invalid selection. Please enter a number between 1 and {}.",
                            ipv4_addrs.len()
                        );
                    }
                }
            }
        }
    }
}

// Generates a default configuration file after prompting the user to select an IP address.
pub fn generate_default_config<P: AsRef<Path>>(path: P) {
    let selected_ip = select_ip_address();
    let cert_path = "cert.crt";
    let key_path = "cert.key";

    // Generate the certificate and key using the selected IP.
    println!(
        "> Generating certificate '{}' and key '{}' for IP address {}...",
        cert_path, key_path, selected_ip
    );
    generate_certificate(cert_path, key_path, &selected_ip);
    println!("+ Certificate and key generated successfully.");

    let uuid = Uuid::new_v4();
    let content = format!(
        r#"[setup]
mode = "server"
certificate = "{}"
private_key = "{}"
auth_token = "{}"
log_level = "info"

[network]
listen = "0.0.0.0"
address = "{}"
port = 33321

[[rfs]]
dev_name = "ipel_disk_1"
bind_path = "/path/to/your/volume/folder1"

[[rfs]]
dev_name = "ipel_disk_2"
bind_path = "/path/to/your/volume/folder2"
"#,
        cert_path, key_path, uuid, selected_ip
    );

    let mut file = File::create(path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    println!("+ Default configuration file created successfully.");
}