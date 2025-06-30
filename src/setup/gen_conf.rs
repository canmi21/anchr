/* src/setup/gen_conf.rs */

use std::fs::File;
use std::io::Write;
use std::path::Path;
use uuid::Uuid;

pub fn generate_default_config<P: AsRef<Path>>(path: P) {
    let uuid = Uuid::new_v4();
    let content = format!(
        r#"[setup]
mode = "server"
certificate = "cert.crt"
private_key = "cert.key"
auth_token = "{}"

[network]
listen = "0.0.0.0"
address = "127.0.0.1"
port = 33321
"#,
        uuid
    );

    let mut file = File::create(path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
}
