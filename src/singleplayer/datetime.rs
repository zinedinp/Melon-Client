//! Local date and time for the world list and the backup file names.

/// Broken-down local time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// `millis` since the epoch in the machine's time zone.
pub fn local(millis: i64) -> Local {
    use chrono::{Datelike, TimeZone, Timelike};
    let time = chrono::Local
        .timestamp_millis_opt(millis)
        .single()
        .unwrap_or_else(chrono::Local::now);
    Local {
        year: time.year(),
        month: time.month(),
        day: time.day(),
        hour: time.hour(),
        minute: time.minute(),
        second: time.second(),
    }
}

/// `LevelStorageAccess.makeWorldBackup`'s `yyyy-MM-dd_HH-mm-ss`.
pub fn backup_stamp(millis: i64) -> String {
    let t = local(millis);
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

/// `Util.localizedDateFormatter(FormatStyle.SHORT)` for the system locale.
pub fn short(millis: i64) -> String {
    format_short(local(millis), &system_locale())
}

fn system_locale() -> String {
    ["LC_ALL", "LC_TIME", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
        .unwrap_or_default()
}

fn format_short(t: Local, locale: &str) -> String {
    let yy = t.year.rem_euclid(100);
    if locale.starts_with("de") {
        format!(
            "{:02}.{:02}.{yy:02}, {:02}:{:02}",
            t.day, t.month, t.hour, t.minute
        )
    } else if locale.is_empty() || locale.starts_with("en_US") || locale == "C" || locale == "POSIX"
    {
        let (hour, suffix) = match t.hour {
            0 => (12, "AM"),
            1..=11 => (t.hour, "AM"),
            12 => (12, "PM"),
            h => (h - 12, "PM"),
        };
        format!(
            "{}/{}/{yy:02}, {hour}:{:02}\u{202f}{suffix}",
            t.month, t.day, t.minute
        )
    } else {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            t.year, t.month, t.day, t.hour, t.minute
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Local = Local {
        year: 2026,
        month: 11,
        day: 9,
        hour: 15,
        minute: 4,
        second: 7,
    };

    #[test]
    fn local_time_is_within_a_day_of_utc() {
        let t = local(1_794_236_647_000);
        assert_eq!((t.year, t.month), (2026, 11));
        assert!((8..=10).contains(&t.day));
        assert_eq!(t.second, 7);
    }

    #[test]
    fn short_formats_follow_the_locale() {
        assert_eq!(format_short(T, "de_DE.UTF-8"), "09.11.26, 15:04");
        assert_eq!(format_short(T, "en_US.UTF-8"), "11/9/26, 3:04\u{202f}PM");
        assert_eq!(format_short(T, "fr_FR"), "2026-11-09 15:04");
        let midnight = Local { hour: 0, ..T };
        assert_eq!(format_short(midnight, ""), "11/9/26, 12:04\u{202f}AM");
    }
}
