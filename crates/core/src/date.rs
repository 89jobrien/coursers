//! Canonical date conversion utilities shared across crs-core, crs, and coursers.
//!
//! All functions are pure (no I/O, no side effects) and operate on Unix timestamps
//! represented as `u64` seconds since the Unix epoch (1970-01-01T00:00:00Z).

// ---------------------------------------------------------------------------
// Named constants
// ---------------------------------------------------------------------------

pub const SECS_PER_MIN: u64 = 60;
pub const SECS_PER_HOUR: u64 = 60 * SECS_PER_MIN;
pub const SECS_PER_DAY: u64 = 24 * SECS_PER_HOUR;

/// Days in a 400-year Gregorian cycle (Rata Die algorithm constant).
pub const DAYS_PER_400Y: u64 = 146_097;
/// Days in a 100-year cycle within a 400-year cycle.
pub const DAYS_PER_100Y: u64 = 36_524;
/// Days in a 4-year leap cycle.
pub const DAYS_PER_4Y: u64 = 1_461;
/// Days in a common (non-leap) year.
pub const DAYS_PER_YEAR: u64 = 365;

// ---------------------------------------------------------------------------
// Shared functions
// ---------------------------------------------------------------------------

/// Convert Unix seconds to `(year, month, day)` using the Rata Die algorithm.
///
/// Fast, branchless for the century/era math; O(1) unlike the naive loop approach.
pub fn unix_secs_to_ymd(secs: u64) -> (u32, u32, u32) {
    let days_since_epoch = secs / SECS_PER_DAY;
    // Shift epoch from 1970-01-01 to 0000-03-01 (civil calendar origin for Rata Die)
    let mut z = days_since_epoch + 719_468;
    let era = z / DAYS_PER_400Y;
    z %= DAYS_PER_400Y;
    let yoe = (z - z / DAYS_PER_4Y + z / DAYS_PER_100Y - z / (DAYS_PER_400Y - 1)) / DAYS_PER_YEAR;
    let y = yoe + era * 400;
    let doy = z - (DAYS_PER_YEAR * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}

/// Convert Unix seconds to `(year, month, day, hour, minute, second)`.
pub fn unix_secs_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let sec = (secs % SECS_PER_MIN) as u32;
    let min = ((secs % SECS_PER_HOUR) / SECS_PER_MIN) as u32;
    let hour = ((secs % SECS_PER_DAY) / SECS_PER_HOUR) as u32;
    let (y, m, d) = unix_secs_to_ymd(secs);
    (y, m, d, hour, min, sec)
}

/// Format Unix seconds as `YYYY-MM-DD`.
pub fn unix_secs_to_date_str(secs: u64) -> String {
    let (y, m, d) = unix_secs_to_ymd(secs);
    format!("{y:04}-{m:02}-{d:02}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_zero_is_1970_01_01() {
        assert_eq!(unix_secs_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn known_date_2024_03_15() {
        // 2024-03-15 00:00:00 UTC = 1710460800
        assert_eq!(unix_secs_to_ymd(1_710_460_800), (2024, 3, 15));
    }

    #[test]
    fn hms_decomposition() {
        // 1970-01-01 01:02:03
        let secs = 3600 + 120 + 3;
        let (y, mo, d, h, mi, s) = unix_secs_to_ymd_hms(secs);
        assert_eq!((y, mo, d), (1970, 1, 1));
        assert_eq!((h, mi, s), (1, 2, 3));
    }

    #[test]
    fn date_str_format() {
        assert_eq!(unix_secs_to_date_str(0), "1970-01-01");
        assert_eq!(unix_secs_to_date_str(1_710_460_800), "2024-03-15");
    }

    #[test]
    fn leap_year_feb_29() {
        // 2000-02-29 is valid (Y2K leap year)
        // 2000-02-29 00:00:00 UTC = 951782400
        assert_eq!(unix_secs_to_ymd(951_782_400), (2000, 2, 29));
    }
}
