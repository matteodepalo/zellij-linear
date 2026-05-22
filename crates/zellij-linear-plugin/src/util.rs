//! Tiny time helpers — kept here to avoid pulling in `chrono`/`time`.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn iso8601_now() -> String {
    iso8601_from_unix(now_unix())
}

/// Format unix seconds as `YYYY-MM-DDTHH:MM:SSZ` using Howard Hinnant's
/// civil_from_days algorithm. Input is `u64`, so anything from the
/// epoch (1970) onward is valid.
pub fn iso8601_from_unix(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let time_of_day = secs % 86400;
    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;
    let second = (time_of_day % 60) as u32;

    // civil_from_days: see https://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_formats_as_1970() {
        assert_eq!(iso8601_from_unix(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn known_timestamps_format_correctly() {
        // Verified against `date -u -r <epoch>`:
        assert_eq!(iso8601_from_unix(946684800), "2000-01-01T00:00:00Z");
        assert_eq!(iso8601_from_unix(1704067200), "2024-01-01T00:00:00Z");
        // Leap day, mid-day.
        assert_eq!(iso8601_from_unix(1709210096), "2024-02-29T12:34:56Z");
    }
}
