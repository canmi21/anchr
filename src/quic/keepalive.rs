/* src/quic/keepalive.rs */

pub const KEEPALIVE_INTERVAL_SECS: u64 = 1;
pub const KEEPALIVE_HEADER: &str = "PING";
pub const KEEPALIVE_ACK_HEADER: &str = "PONG";
