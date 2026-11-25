//! The document that `flare metrics` prints.

use crumbkit::{Document, Listing, Row};
use schemars::JsonSchema;
use serde::Serialize;

/// The count of one metric on one day in `flare metrics`.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct MetricRow {
    /// The day, as `YYYY-MM-DD` in UTC.
    #[row(column)]
    pub(crate) day: String,
    /// The metric's name, such as `PagesViewed`.
    #[row(column)]
    pub(crate) metric: String,
    /// The number of events the metric counted that day.
    #[row(column)]
    pub(crate) count: u64,
}

/// The count of one metric over every day in `flare metrics`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct MetricTotal {
    /// The metric's name.
    pub(crate) metric: String,
    /// The number of events the metric counted.
    pub(crate) count: u64,
}

/// The totals of `flare metrics` and the lines it read.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct MetricTotals {
    /// The count of each metric over every day, ordered by name.
    pub(crate) totals: Vec<MetricTotal>,
    /// The number of lines read.
    pub(crate) lines: u64,
    /// The number of lines skipped because they are not Flare metric lines.
    pub(crate) skipped: u64,
}

impl std::fmt::Display for MetricTotals {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for total in &self.totals {
            writeln!(formatter, "{}: {}", total.metric, total.count)?;
        }
        write!(
            formatter,
            "Read {} lines and skipped {} that are not Flare metric lines.",
            self.lines, self.skipped
        )
    }
}

/// The document of `flare metrics`.
pub(crate) const METRICS: Document<Listing<MetricRow, MetricTotals>> = Document::new(
    "flare",
    "metrics",
    1,
    "The count of each Flare metric on each day in UTC, the total of each metric, and the lines read and skipped.",
)
.when_empty("No Flare metric lines.");
