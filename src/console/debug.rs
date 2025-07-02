/* src/console/debug.rs */

use crate::console::app::Stats;
use std::sync::atomic::Ordering;

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_stats(stats: &Stats) -> String {
    let tx = stats.tx_bytes.load(Ordering::Relaxed);
    let rx = stats.rx_bytes.load(Ordering::Relaxed);
    let last_id = stats.last_msg_id.load(Ordering::Relaxed);
    let pool_count = stats.pool_count.load(Ordering::Relaxed);

    format!(
        "tx: {} | rx: {} | c:{} | p:{}",
        format_bytes(tx),
        format_bytes(rx),
        last_id,
        pool_count
    )
}