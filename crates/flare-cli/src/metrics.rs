//! `flare metrics`: totals the Flare metric lines of a server's log, by day
//! in UTC and by metric, with flare-server's [`Totals`].
//!
//! It reads every file it is given in order, or standard input without a
//! file, and skips each line that is not a Flare EMF line. A line that is
//! not UTF-8 is read with its bad bytes replaced, so it counts as skipped.
//! docs/metrics.md describes the lines.

use crate::cli::MetricsArgs;
use crate::codes::Failed;
use crate::documents::{MetricRow, MetricTotal, MetricTotals};
use crumbkit::Listing;
use flare_server::Totals;
use std::io::{BufRead, BufReader, IsTerminal as _};

/// Reads the logs that `args` names and totals their metric lines.
///
/// # Errors
///
/// Returns [`Failed::MetricsUnreadable`] when a file or standard input
/// cannot be read, or when standard input is a terminal.
pub(crate) fn run(args: &MetricsArgs) -> Result<Listing<MetricRow, MetricTotals>, Failed> {
    let mut totals = Totals::default();
    if args.files.is_empty() {
        let shown = "standard input";
        // crumbkit reads standard input only to answer a question, so this
        // command reads its piped log itself.
        #[expect(
            clippy::disallowed_methods,
            reason = "flare metrics reads a piped log from standard input, which crumbkit has no reader for"
        )]
        let input = std::io::stdin();
        if input.is_terminal() {
            return Err(Failed::MetricsUnreadable {
                path: shown.to_owned(),
                problem: "standard input is a terminal".to_owned(),
            });
        }
        read(input.lock(), shown, &mut totals)?;
    }
    for file in &args.files {
        let shown = file.display().to_string();
        let opened = std::fs::File::open(file).map_err(|error| Failed::MetricsUnreadable {
            path: shown.clone(),
            problem: error.to_string(),
        })?;
        read(BufReader::new(opened), &shown, &mut totals)?;
    }
    let rows: Vec<MetricRow> = totals
        .by_day()
        .map(|(day, metric, count)| MetricRow {
            day: day.to_owned(),
            metric: metric.to_owned(),
            count,
        })
        .collect();
    let summary = MetricTotals {
        totals: totals
            .by_metric()
            .map(|(metric, count)| MetricTotal {
                metric: metric.to_owned(),
                count,
            })
            .collect(),
        lines: totals.lines(),
        skipped: totals.skipped(),
    };
    let total = rows.len();
    Ok(Listing::new(rows, total)
        .described("metric counts")
        .summary(summary))
}

/// Adds every line of `input`, named `shown` in a failure, to `totals`.
///
/// Shortcut: a line is read whole into memory, however long it is. A
/// server's log lines are short; a reader of arbitrary files would cap the
/// line length and skip what is past the cap.
fn read(mut input: impl BufRead, shown: &str, totals: &mut Totals) -> Result<(), Failed> {
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        let read =
            input
                .read_until(b'\n', &mut bytes)
                .map_err(|error| Failed::MetricsUnreadable {
                    path: shown.to_owned(),
                    problem: error.to_string(),
                })?;
        if read == 0 {
            return Ok(());
        }
        totals.add(&String::from_utf8_lossy(&bytes));
    }
}
