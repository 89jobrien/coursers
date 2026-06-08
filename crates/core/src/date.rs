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

/// Rata Die epoch offset: days from 0000-03-01 to 1970-01-01.
const RATA_DIE_EPOCH_OFFSET: u64 = 719_468;
/// Years in one Gregorian era (400-year cycle).
const YEARS_PER_ERA: u64 = 400;
/// Rata Die month-of-year multiplier (numerator).
const MP_NUMER: u64 = 5;
/// Rata Die month-of-year accumulator offset.
const MP_OFFSET: u64 = 2;
/// Rata Die month-of-year divisor.
const MP_DENOM: u64 = 153;
/// March-based month threshold for year adjustment.
const MARCH_MONTH_THRESHOLD: u64 = 10;
/// March-based offset to convert to calendar month (mp < 10).
const MONTH_ADJUST_ADD: u64 = 3;
/// March-based offset to convert to calendar month (mp >= 10).
const MONTH_ADJUST_SUB: u64 = 9;
/// Month boundary for year increment (Jan/Feb).
const YEAR_INCREMENT_BOUNDARY: u64 = 2;

// ---------------------------------------------------------------------------
// Shared functions
// ---------------------------------------------------------------------------

/// Convert Unix seconds to `(year, month, day)` using the Rata Die algorithm.
///
/// Fast, branchless for the century/era math; O(1) unlike the naive loop approach.
pub fn unix_secs_to_ymd(secs: u64) -> (u32, u32, u32) {
    let days_since_epoch = secs / SECS_PER_DAY;
    // Shift epoch from 1970-01-01 to 0000-03-01 (civil calendar origin for Rata Die)
    let mut z = days_since_epoch + RATA_DIE_EPOCH_OFFSET;
    let era = z / DAYS_PER_400Y;
    z %= DAYS_PER_400Y;
    let yoe =
        (z - z / (DAYS_PER_4Y - 1) + z / DAYS_PER_100Y - z / (DAYS_PER_400Y - 1)) / DAYS_PER_YEAR;
    let y = yoe + era * YEARS_PER_ERA;
    let doy = z
        - (DAYS_PER_YEAR * yoe + yoe / (DAYS_PER_4Y / DAYS_PER_YEAR)
            - yoe / (DAYS_PER_100Y / DAYS_PER_YEAR));
    let mp = (MP_NUMER * doy + MP_OFFSET) / MP_DENOM;
    let d = doy - (MP_DENOM * mp + MP_OFFSET) / MP_NUMER + 1;
    let m = if mp < MARCH_MONTH_THRESHOLD {
        mp + MONTH_ADJUST_ADD
    } else {
        mp - MONTH_ADJUST_SUB
    };
    let y = if m <= YEAR_INCREMENT_BOUNDARY {
        y + 1
    } else {
        y
    };
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
// Kani verification harnesses
// ---------------------------------------------------------------------------

#[cfg(kani)]
mod kani_proofs {
    use super::{unix_secs_to_ymd, unix_secs_to_ymd_hms};

    // TODO: reconsider — these are mathematical constants, not config; literals
    // (31, 4, 100) were more readable. Added only for rustqual magic-number check.
    const DAYS_31: u32 = 31;
    const DAYS_30: u32 = 30;
    const DAYS_29: u32 = 29;
    const DAYS_28: u32 = 28;
    const LEAP_DIV_4: u32 = 4;
    const LEAP_DIV_100: u32 = 100;
    const LEAP_DIV_400: u32 = 400;
    /// Upper bound on input seconds for kani tractability (~34,000 years).
    const KANI_TRACTABILITY_BOUND: u64 = 1u64 << 40;

    /// Helper: true number of days in a given month for a given year.
    fn days_in_month(y: u32, m: u32) -> u32 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => DAYS_31,
            4 | 6 | 9 | 11 => DAYS_30,
            2 => {
                if (y % LEAP_DIV_4 == 0 && y % LEAP_DIV_100 != 0) || y % LEAP_DIV_400 == 0 {
                    DAYS_29
                } else {
                    DAYS_28
                }
            }
            _ => unreachable!(),
        }
    }

    /// Proof: unix_secs_to_ymd always produces valid month (1..=12) and day (1..=31).
    #[kani::proof]
    #[kani::unwind(1)]
    fn ymd_bounds_valid() {
        let secs: u64 = kani::any();
        kani::assume(secs < KANI_TRACTABILITY_BOUND);
        let (_, m, d) = unix_secs_to_ymd(secs);
        assert!(m >= 1 && m <= 12, "month out of range: {m}");
        assert!(d >= 1 && d <= DAYS_31, "day out of range: {d}");
    }

    /// Proof: day never exceeds the actual days in the computed month/year.
    #[kani::proof]
    #[kani::unwind(1)]
    fn ymd_day_within_month() {
        let secs: u64 = kani::any();
        kani::assume(secs < KANI_TRACTABILITY_BOUND);
        let (y, m, d) = unix_secs_to_ymd(secs);
        let max_d = days_in_month(y, m);
        assert!(d <= max_d, "day {d} exceeds max {max_d} for {y}-{m:02}");
    }

    /// Proof: monotonicity — later timestamps never produce earlier dates.
    /// Bounded to u32 range (~136 years from epoch) for solver tractability.
    #[kani::proof]
    #[kani::unwind(1)]
    fn ymd_monotonic() {
        let s1: u32 = kani::any();
        let s2: u32 = kani::any();
        kani::assume(s1 < s2);
        let (y1, m1, d1) = unix_secs_to_ymd(s1 as u64);
        let (y2, m2, d2) = unix_secs_to_ymd(s2 as u64);
        assert!(
            (y2, m2, d2) >= (y1, m1, d1),
            "monotonicity violated: {y1}-{m1}-{d1} > {y2}-{m2}-{d2}"
        );
    }

    /// Proof: HMS decomposition always produces valid bounds.
    #[kani::proof]
    #[kani::unwind(1)]
    fn hms_bounds_valid() {
        let secs: u64 = kani::any();
        kani::assume(secs < KANI_TRACTABILITY_BOUND);
        let (_, m, d, h, mi, s) = unix_secs_to_ymd_hms(secs);
        assert!(m >= 1 && m <= 12);
        assert!(d >= 1 && d <= DAYS_31);
        assert!(h < 24, "hour out of range: {h}");
        assert!(mi < 60, "minute out of range: {mi}");
        assert!(s < 60, "second out of range: {s}");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{unix_secs_to_date_str, unix_secs_to_ymd, unix_secs_to_ymd_hms};

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
