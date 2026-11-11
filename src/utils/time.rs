use std::time::{SystemTime, UNIX_EPOCH};

pub fn format_time_now() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{:02}:{:02}:{:02}.{:03} ", 0, 0, 0, millis % 1000)
}
