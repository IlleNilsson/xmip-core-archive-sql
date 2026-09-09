//! The moment an item is archived, as RFC 3339 in UTC — `2026-09-09T12:00:00Z` —
//! computed from [`SystemTime`] by hand, so the crate carries no calendar
//! dependency for the sake of one column.

use std::time::{SystemTime, UNIX_EPOCH};

/// Now, as `archived_at` is written.
#[must_use]
pub fn now() -> String {
    rfc3339(SystemTime::now())
}

/// `at` as RFC 3339 in UTC to the whole second. A moment before the epoch,
/// which no clock this runs under reports, is written as the epoch itself.
#[must_use]
pub fn rfc3339(at: SystemTime) -> String {
    let secs = at
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0);
    let (year, month, day) = civil(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Year, month and day of a day count since 1970-01-01 — Howard Hinnant's
/// civil-from-days.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_epoch_is_the_first_moment_of_1970() {
        assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn a_known_instant_is_written_as_rfc_3339() {
        let at = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        assert_eq!(rfc3339(at), "2020-09-13T12:26:40Z");
    }

    #[test]
    fn a_leap_day_is_a_real_day() {
        let at = UNIX_EPOCH + Duration::from_secs(1_709_164_800);
        assert_eq!(rfc3339(at), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn now_has_the_shape_of_a_timestamp() {
        let text = now();
        assert_eq!(text.len(), 20);
        assert!(text.ends_with('Z'));
        assert_eq!(&text[10..11], "T");
    }
}
