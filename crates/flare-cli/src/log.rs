//! The server's structured log on standard error (SVC-9). crumbkit's
//! output owns every other write to the standard streams (CLI-1); this
//! module owns the log that `flare serve` writes while it runs, which
//! flare-server's request lines go to.
//!
//! A metric line, an event with flare-server's [`METRICS_TARGET`], is
//! written as its bare JSON object, because CloudWatch reads only a log
//! event that is one JSON object. Every other event is written by
//! tracing-subscriber's default format, as it was before metrics existed.

use flare_server::METRICS_TARGET;
use std::fmt::{self, Write as _};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::{Format, Writer};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

#[cfg(test)]
mod tests;

/// Sends `tracing` events to standard error, one line each, without
/// colour, for the rest of the process.
pub(crate) fn start() {
    let text = Format::default().with_ansi(false).with_target(false);
    #[expect(
        clippy::disallowed_methods,
        reason = "the server's log is the one writer to standard error outside crumbkit's output (CLI-1)"
    )]
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .event_format(Lines::new(text))
        .with_writer(std::io::stderr)
        .finish();
    // A second call keeps the first subscriber, which writes to the same
    // stream.
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Writes a metric event as its message alone, and hands every other
/// event to `text`.
pub(crate) struct Lines<T> {
    text: T,
}

impl<T> Lines<T> {
    /// Lines that write every event but a metric one with `text`.
    pub(crate) fn new(text: T) -> Self {
        Self { text }
    }
}

impl<S, N, T> FormatEvent<S, N> for Lines<T>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
    T: FormatEvent<S, N>,
{
    fn format_event(
        &self,
        context: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        if event.metadata().target() != METRICS_TARGET {
            return self.text.format_event(context, writer, event);
        }
        let mut message = Message::default();
        event.record(&mut message);
        writer.write_str(&message.0)?;
        writer.write_char('\n')
    }
}

/// The `message` field of an event, as its text.
#[derive(Default)]
struct Message(String);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        }
    }
}
