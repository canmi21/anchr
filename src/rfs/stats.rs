/* src/rfs/stats.rs */

use crate::rfs::UploadContext;
use log::info;

/// Formats file size and speed with appropriate units (KB, MB, GB, etc.).
fn format_speed_and_size(bytes: u64, duration: std::time::Duration) -> (String, String) {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * KB;
    const GB: f64 = 1024.0 * MB;

    let size_str = if bytes as f64 >= GB {
        format!("{:.2} GB", bytes as f64 / GB)
    } else if bytes as f64 >= MB {
        format!("{:.2} MB", bytes as f64 / MB)
    } else if bytes as f64 >= KB {
        format!("{:.2} KB", bytes as f64 / KB)
    } else {
        format!("{} Bytes", bytes)
    };

    let seconds = duration.as_secs_f64();
    if seconds < 1e-6 { // Avoid division by zero if duration is negligible
        return (size_str, "N/A".to_string());
    }

    let speed = bytes as f64 / seconds;
    let speed_str = if speed >= GB {
        format!("{:.2} GB/s", speed / GB)
    } else if speed >= MB {
        format!("{:.2} MB/s", speed / MB)
    } else if speed >= KB {
        format!("{:.2} KB/s", speed / KB)
    } else {
        format!("{:.0} B/s", speed)
    };

    (size_str, speed_str)
}

/// Calculates and logs the final upload statistics.
pub fn log_completion_stats(ctx: &UploadContext) {
    let duration = ctx.start_time.elapsed();
    let (size_str, speed_str) = format_speed_and_size(ctx.metadata.file_size, duration);
    // The success check happens in the calling function, so we just log the stats here.
    info!(
        "   Total time: {:.2?}, File size: {}, Average speed: {}",
        duration, size_str, speed_str
    );
}