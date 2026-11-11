use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeParts {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub milliseconds: u32,
}

pub fn format_time(time: TimeParts) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:<3}",
        time.hours, time.minutes, time.seconds, time.milliseconds
    )
}

pub fn format_time_now() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = duration.as_secs();
    let millis = duration.subsec_millis();
    format!(
        "{:02}:{:02}:{:02}.{:<3}",
        (seconds / 3600) % 24,
        (seconds / 60) % 60,
        seconds % 60,
        millis
    )
}

pub fn get_current_microseconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::{format_time, TimeParts};

    #[test]
    fn should_correctly_format_a_timestamp_with_microseconds_padding_with_spaces() {
        let time = TimeParts {
            hours: 4,
            minutes: 55,
            seconds: 2,
            milliseconds: 12,
        };
        assert_eq!(format_time(time), "04:55:02.12 ");
    }
}
