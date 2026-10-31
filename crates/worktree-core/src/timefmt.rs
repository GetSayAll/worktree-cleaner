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

/// Parses `YYYY-MM-DDTHH:MM:SSZ` back into an instant.
///
/// The only timestamps this core reads back are ones it wrote itself — HEAD commit
/// times and activity fields of its own snapshot — so a strict fixed-width parse of
/// exactly that format is the honest contract: anything else is `None`, which a
/// caller must treat as "not measured" rather than as an old timestamp (D3).
pub fn parse_rfc3339(text: &str) -> Option<SystemTime> {
    let bytes = text.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: u32 = text.get(5..7)?.parse().ok()?;
    let day: u32 = text.get(8..10)?.parse().ok()?;
    let hour: i128 = text.get(11..13)?.parse().ok()?;
    let minute: i128 = text.get(14..16)?.parse().ok()?;
    let second: i128 = text.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(from_unix_seconds(
        days * SECONDS_PER_DAY + hour * 3600 + minute * 60 + second,
    ))
}

/// Days since 1970-01-01 for a civil date. The inverse of [`civil_from_days`].
const fn days_from_civil(year: i64, month: u32, day: u32) -> i128 {
    let year = year as i128;
    let month = month as i128;
    let day = day as i128;
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400; // [0, 399]
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year; // [0, 146096]
    era * 146_097 + day_of_era - 719_468
}

/// Seconds since the Unix epoch as a signed value, so pre-epoch times work.
fn unix_seconds(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(delta) => i128::from(delta.as_secs()),
        Err(error) => -i128::from(error.duration().as_secs()),
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
const fn civil_from_days(days_since_epoch: i128) -> (i64, u32, u32) {
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
            assert_eq!(
                parsed,
                from_unix_seconds(seconds),
                "round trip failed for {seconds}"
            );
        }
    }

    #[test]
    fn parse_rejects_anything_it_did_not_write() {
        // A strict parser is the point: a timestamp it cannot read is "not measured",
        // and a lenient one would answer with a wrong date instead (D3).
        for text in [
            "",
            "2026-01-01",
            "2026-01-01T00:00:00",
            "2026-13-01T00:00:00Z",
            "2026-01-32T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2026-01-01T00:00:00+00:00",
            "not a timestamp at all",
        ] {
            assert_eq!(parse_rfc3339(text), None, "{text} should not parse");
        }
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
