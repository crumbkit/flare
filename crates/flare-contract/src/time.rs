//! Moments as RFC 3339 text in UTC, such as `2026-11-23T09:12:04Z`.
//!
//! Shortcut: this writes whole seconds in UTC only, with Howard Hinnant's
//! days-to-civil algorithm, and parses nothing. It handles every moment
//! from 1970 onward that a `u64` of seconds holds up to the year 9999. A
//! caller that needs time zones, parsing or arithmetic adopts `jiff`
//! after its evaluation.

/// The seconds in one day.
const DAY: u64 = 86_400;

/// The moment `seconds` after the Unix epoch, as RFC 3339 text in UTC.
#[must_use]
pub fn utc(seconds: u64) -> String {
    let days = seconds / DAY;
    let rest = seconds % DAY;
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The year, month and day of `days` after 1970-01-01.
fn civil(days: u64) -> (u64, u64, u64) {
    // Shift the epoch to 0000-03-01, so a leap day ends each 4-year cycle.
    let z = days.saturating_add(719_468);
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests;
