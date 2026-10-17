//! Dates the way GitHub writes them: "3 hours ago", "yesterday", "on Sep 3".

use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

pub fn relative(moment: Timestamp, now: Timestamp, zone: &TimeZone) -> String {
    let seconds = (now.as_nanosecond() - moment.as_nanosecond()) as f64 / 1e9;
    if seconds < 45.0 {
        return "just now".into();
    }
    let minutes = seconds / 60.0;
    if minutes < 1.5 {
        return "1 minute ago".into();
    }
    if minutes < 45.0 {
        return format!("{} minutes ago", minutes.round_ties_even());
    }
    let hours = minutes / 60.0;
    if hours < 1.5 {
        return "1 hour ago".into();
    }
    if hours < 22.0 {
        return format!("{} hours ago", hours.round_ties_even());
    }
    let days = hours / 24.0;
    if days < 1.5 {
        return "yesterday".into();
    }
    if days < 26.0 {
        return format!("{} days ago", days.round_ties_even());
    }
    let local = moment.to_zoned(zone.clone());
    let this_year = now.to_zoned(zone.clone()).year();
    let year = if local.year() != this_year {
        format!(", {}", local.year())
    } else {
        String::new()
    };
    format!("on {} {}{year}", local.strftime("%b"), local.day())
}

pub fn absolute(moment: Timestamp, zone: &TimeZone) -> String {
    let local = moment.to_zoned(zone.clone());
    local
        .strftime("%b %-d, %Y, %H:%M %Z")
        .to_string()
        .trim()
        .to_string()
}

/// A timestamp written without a zone, read as local time in `zone`.
pub fn local(naive: DateTime, zone: &TimeZone) -> Option<Zoned> {
    naive.to_zoned(zone.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::SignedDuration;
    use jiff::civil::date;

    fn utc(year: i16, month: i8, day: i8, hour: i8) -> Timestamp {
        date(year, month, day)
            .at(hour, 0, 0, 0)
            .to_zoned(TimeZone::UTC)
            .unwrap()
            .timestamp()
    }

    #[test]
    fn relative_times_read_like_github() {
        let now = utc(2026, 9, 24, 12);
        let zone = TimeZone::UTC;
        let ago = |seconds: i64| relative(now - SignedDuration::from_secs(seconds), now, &zone);
        assert_eq!(ago(10), "just now");
        assert_eq!(ago(5 * 60), "5 minutes ago");
        assert_eq!(ago(3 * 3600), "3 hours ago");
        assert_eq!(ago(86400), "yesterday");
        assert_eq!(ago(4 * 86400), "4 days ago");
        assert_eq!(relative(utc(2026, 3, 2, 0), now, &zone), "on Mar 2");
        assert_eq!(relative(utc(2025, 3, 2, 0), now, &zone), "on Mar 2, 2025");
    }

    #[test]
    fn the_day_is_the_day_in_the_configured_zone() {
        let now = utc(2026, 9, 24, 12);
        let zone = TimeZone::get("Europe/Warsaw").unwrap();
        assert_eq!(relative(utc(2026, 3, 1, 23), now, &zone), "on Mar 2");
    }

    #[test]
    fn absolute_times_are_shown_in_the_configured_zone() {
        let zone = TimeZone::get("Europe/Warsaw").unwrap();
        assert_eq!(
            absolute(utc(2026, 9, 24, 12), &zone),
            "Sep 24, 2026, 14:00 CEST"
        );
    }
}
