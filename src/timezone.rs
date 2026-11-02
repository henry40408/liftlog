//! Per-user timezone handling. `workout_sessions.date` is a calendar date in
//! the user's own timezone, so "today" and rolling windows must be computed
//! there, not in UTC or the server's local zone.

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;

/// Parses an IANA name such as `Asia/Taipei`.
pub fn parse(name: &str) -> Option<Tz> {
    name.parse().ok()
}

/// The calendar date "now" in the given zone. `None` (not chosen yet) or an
/// unknown name (e.g. removed from the IANA database) falls back to UTC.
pub fn today_in(timezone: Option<&str>) -> NaiveDate {
    date_at(Utc::now(), timezone)
}

fn date_at(instant: DateTime<Utc>, timezone: Option<&str>) -> NaiveDate {
    let tz = timezone.and_then(parse).unwrap_or(Tz::UTC);
    instant.with_timezone(&tz).date_naive()
}

/// First date of a window that covers `days` calendar days ending today.
pub fn window_start(today: NaiveDate, days: u32) -> NaiveDate {
    today - chrono::Duration::days(i64::from(days) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    #[test]
    fn parse_accepts_iana_names_only() {
        assert!(parse("Asia/Taipei").is_some());
        assert!(parse("America/New_York").is_some());
        assert!(parse("Not/AZone").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn date_at_follows_the_zone_across_the_utc_date_line() {
        // 20:00 UTC on 11-08 is already 11-09 in Taipei but still 11-08 in LA.
        let instant = "2026-11-08T20:00:00Z".parse().unwrap();
        assert_eq!(date_at(instant, Some("Asia/Taipei")), date(11, 9));
        assert_eq!(date_at(instant, Some("America/Los_Angeles")), date(11, 8));
    }

    #[test]
    fn date_at_falls_back_to_utc_when_unset_or_unknown() {
        let instant = "2026-11-08T20:00:00Z".parse().unwrap();
        assert_eq!(date_at(instant, None), date(11, 8));
        assert_eq!(date_at(instant, Some("Not/AZone")), date(11, 8));
    }

    #[test]
    fn window_start_counts_today_as_the_first_day() {
        let today = date(11, 9);
        assert_eq!(window_start(today, 7), date(11, 3));
        assert_eq!(window_start(today, 1), today);
    }
}
