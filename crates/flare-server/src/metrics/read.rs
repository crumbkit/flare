//! [`Totals`]: the counts of the Flare EMF lines in a log, by day in UTC
//! and by metric. `flare metrics` reads a log with it, and the tests read
//! the lines a run wrote with it.

use super::NAMESPACE;
use serde_json::Value;
use std::collections::BTreeMap;

/// The counts of the Flare EMF lines added so far.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Totals {
    /// The count of each metric on each day, keyed by the day as
    /// `YYYY-MM-DD` in UTC and the metric's name.
    days: BTreeMap<(String, String), u64>,
    lines: u64,
    skipped: u64,
}

impl Totals {
    /// Counts `line` when it is a JSON object whose `_aws` directive names
    /// the `Flare` namespace, and skips it otherwise. A skipped line is
    /// one that is not JSON, is truncated, has no `_aws` object or
    /// `Timestamp`, or names only other namespaces.
    ///
    /// Shortcut: this reads only what flare writes, an integer value at the
    /// root for each metric. An EMF line of another program whose value is
    /// an array of values counts nothing for that metric. A reader of
    /// other programs' EMF lines would need the rest of the specification.
    pub fn add(&mut self, line: &str) {
        self.lines = self.lines.saturating_add(1);
        let counted = counts(line);
        if counted.is_empty() {
            self.skipped = self.skipped.saturating_add(1);
            return;
        }
        for (day, metric, count) in counted {
            let total = self.days.entry((day, metric)).or_default();
            *total = total.saturating_add(count);
        }
    }

    /// The count of each metric on each day, as the day, the metric's name
    /// and the count, ordered by day and then by name.
    pub fn by_day(&self) -> impl Iterator<Item = (&str, &str, u64)> {
        self.days
            .iter()
            .map(|((day, metric), count)| (day.as_str(), metric.as_str(), *count))
    }

    /// The count of each metric over every day, ordered by name.
    pub fn by_metric(&self) -> impl Iterator<Item = (&str, u64)> {
        let mut all: BTreeMap<&str, u64> = BTreeMap::new();
        for ((_, metric), count) in &self.days {
            let total = all.entry(metric.as_str()).or_default();
            *total = total.saturating_add(*count);
        }
        all.into_iter()
    }

    /// The number of lines added, counted or skipped.
    #[must_use]
    pub fn lines(&self) -> u64 {
        self.lines
    }

    /// The number of lines skipped because they are not Flare EMF lines.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }
}

/// The day, metric and count of each Flare metric in `line`, or none.
fn counts(line: &str) -> Vec<(String, String, u64)> {
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    let Some(aws) = root.get("_aws") else {
        return Vec::new();
    };
    let Some(day) = aws
        .get("Timestamp")
        .and_then(Value::as_u64)
        .and_then(|millis| {
            flare_contract::utc(millis / 1000)
                .get(..10)
                .map(str::to_owned)
        })
    else {
        return Vec::new();
    };
    let directives = aws
        .get("CloudWatchMetrics")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    directives
        .iter()
        .filter(|directive| directive.get("Namespace").and_then(Value::as_str) == Some(NAMESPACE))
        .filter_map(|directive| directive.get("Metrics").and_then(Value::as_array))
        .flatten()
        .filter_map(|metric| metric.get("Name").and_then(Value::as_str))
        .filter_map(|name| {
            let count = root.get(name).and_then(Value::as_u64)?;
            Some((day.clone(), name.to_owned(), count))
        })
        .collect()
}
