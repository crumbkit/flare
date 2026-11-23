//! The `--idle-exit` value: a whole number of seconds, minutes, hours or
//! days, such as `90s`, `30m`, `8h` or `2d`.
//!
//! This reads one number and one unit and no more (RUST-10). It takes no
//! fractions and no sums such as `1h30m`; jiff's friendly durations, which
//! crumbkit-usage already brings in, replace it if `flare` needs either.

use std::time::Duration;

#[cfg(test)]
mod tests;

/// Parses `text` as a duration longer than zero.
///
/// # Errors
///
/// Returns why the text is not a duration, for clap to print.
pub(crate) fn parse(text: &str) -> Result<Duration, String> {
    let refused = || format!("{text:?} is not a duration such as 90s, 30m, 8h or 2d");
    let split = text.len().checked_sub(1).ok_or_else(refused)?;
    let (number, unit) = text.split_at_checked(split).ok_or_else(refused)?;
    let seconds_per = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return Err(refused()),
    };
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(refused());
    }
    let count: u64 = number.parse().map_err(|_| refused())?;
    let seconds = count.checked_mul(seconds_per).ok_or_else(refused)?;
    if seconds == 0 {
        return Err(refused());
    }
    Ok(Duration::from_secs(seconds))
}
