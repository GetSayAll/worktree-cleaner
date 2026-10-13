//! Time helpers: formatting and day arithmetic.
//!
//! The snapshot contract carries ISO-8601 UTC timestamps (`last_updated_at`), and the
//! activity window is measured in whole days (product plan section 5.1). Both are
//! implemented here without a date library: the conversion is the classic
//! days-from-civil / civil-from-days pair, pinned by tests against known values so a
//! silent off-by-one cannot reach a safety decision.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: i128 = 86_400;

/// Formats a [`SystemTime`] as `YYYY-MM-DDTHH:MM:SSZ` (UTC).
///
/// Times before the Unix epoch are supported: the day arithmetic uses Euclidean
/// division, so one second before the epoch formats as `1969-12-31T23:59:59Z`.
pub fn format_rfc3339(time: SystemTime) -> String {
    let seconds = unix_seconds(time);
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = seconds.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Whole days between `then` and `now`, saturating at 0 when `then` is in the future
/// (a clock skew must not turn into a negative age).
pub fn elapsed_days(now: SystemTime, then: SystemTime) -> u64 {
    let delta = unix_seconds(now) - unix_seconds(then);
    if delta <= 0 {
        0
    } else {
        (delta / SECONDS_PER_DAY) as u64
    }
}

/// Evidence wording for the activity reason: `today`, `1 day ago`, `N days ago`.
pub fn humanize_days(days: u64) -> String {
    match days {
        0 => "today".to_string(),
        1 => "1 day ago".to_string(),
        count => format!("{count} days ago"),
    }
}

/// Seconds since the Unix epoch as a signed value, so pre-epoch times work.
fn unix_seconds(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(delta) => delta.as_secs() as i128,
        Err(error) => -(error.duration().as_secs() as i128),
    }
}

/// Inverse of [`unix_seconds`]: builds a [`SystemTime`] from signed epoch seconds.
///
/// Used to turn `git log --format=%ct` output into a comparable instant.
pub fn from_unix_seconds(seconds: i128) -> SystemTime {
    if seconds >= 0 {
        UNIX_EPOCH + Duration::from_secs(seconds as u64)
    } else {
        UNIX_EPOCH - Duration::from_secs((-seconds) as u64)
    }
}

/// Days since 1970-01-01 to a `(year, month, day)` triple (Howard Hinnant's
/// `civil_from_days`). All divisions are Euclidean, which is what makes the
/// pre-epoch cases come out right.
fn civil_from_days(days_since_epoch: i128) -> (i64, u32, u32) {
    let era_epoch = days_since_epoch + 719_468;
    let era = era_epoch.div_euclid(146_097);
    let day_of_era = era_epoch.rem_euclid(146_097); // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // [0, 365]
    let month_index = (5 * day_of_year + 2) / 153; // [0, 11]
    let day = day_of_year - (153 * month_index + 2) / 5 + 1; // [1, 31]
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    }; // [1, 12]
    let year = if month <= 2 { year + 1 } else { year };
    (year as i64, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i128) -> SystemTime {
        if seconds >= 0 {
            UNIX_EPOCH + Duration::from_secs(seconds as u64)
        } else {
            UNIX_EPOCH - Duration::from_secs((-seconds) as u64)
        }
    }

    /// The inverse of [`civil_from_days`], kept here so the round trips below are
    /// real checks rather than tautologies.
    fn days_from_civil(year: i64, month: u32, day: u32) -> i128 {
        let year = i128::from(year);
        let month = i128::from(month);
        let day = i128::from(day);
        let year = if month <= 2 { year - 1 } else { year };
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400; // [0, 399]
        let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year; // [0, 146096]
        era * 146_097 + day_of_era - 719_468
    }

    #[test]
    fn epoch_formats_as_expected() {
        assert_eq!(format_rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn known_timestamps_format_exactly() {
        // 2026-01-01T00:00:00Z and 2024-02-29T12:34:56Z (a leap day with a time part).
        assert_eq!(format_rfc3339(at(1_767_225_600)), "2026-01-01T00:00:00Z");
        assert_eq!(format_rfc3339(at(1_709_210_096)), "2024-02-29T12:34:56Z");
    }

    #[test]
    fn from_unix_seconds_round_trips_through_formatting() {
        for seconds in [-1_i128, 0, 1, 1_700_000_000, 1_767_225_600] {
            let formatted = format_rfc3339(from_unix_seconds(seconds));
            let parsed = parse_rfc3339(&formatted).expect("parse back");
            assert_eq!(parsed, seconds, "round trip failed for {seconds}");
        }
    }

    /// Minimal parser for the format [`format_rfc3339`] produces, used only to prove
    /// the round trip.
    fn parse_rfc3339(text: &str) -> Option<i128> {
        let (date, time) = text.split_once('T')?;
        let (year, rest) = date.split_once('-')?;
        let (month, day) = rest.split_once('-')?;
        let (hour, rest) = time.split_once(':')?;
        let (minute, second) = rest.split_once(':')?;
        let second = second.strip_suffix('Z')?;
        let days = days_from_civil(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?);
        let seconds: i128 = hour.parse().ok()?;
        let seconds =
            seconds * 3600 + minute.parse::<i128>().ok()? * 60 + second.parse::<i128>().ok()?;
        Some(days * 86_400 + seconds)
    }

    #[test]
    fn one_second_before_the_epoch() {
        assert_eq!(format_rfc3339(at(-1)), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn day_arithmetic_round_trips() {
        for days in [-40_000_i128, -366, -1, 0, 1, 365, 20_454, 100_000] {
            let (year, month, day) = civil_from_days(days);
            let back = days_from_civil(year, month, day);
            assert_eq!(back, days, "round trip failed for day {days}");
        }
    }

    #[test]
    fn elapsed_days_saturates_in_both_directions() {
        let now = at(10 * SECONDS_PER_DAY);
        assert_eq!(elapsed_days(now, at(3 * SECONDS_PER_DAY)), 7);
        assert_eq!(elapsed_days(now, at(3 * SECONDS_PER_DAY + 86_399)), 6);
        // A future timestamp must not become a negative age.
        assert_eq!(elapsed_days(now, at(11 * SECONDS_PER_DAY)), 0);
    }

    #[test]
    fn humanize_days_reads_naturally() {
        assert_eq!(humanize_days(0), "today");
        assert_eq!(humanize_days(1), "1 day ago");
        assert_eq!(humanize_days(3), "3 days ago");
    }
}
