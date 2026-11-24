use super::Lines;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::format::Format;

/// A log writer that keeps every byte for the test to read.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What the server's log writes for a `served` line and a metric line,
/// through a formatter built by `subscriber`.
fn written(
    subscriber: impl FnOnce(Captured) -> Box<dyn tracing::Subscriber + Send + Sync>,
) -> String {
    let log = Captured::default();
    let _guard = tracing::subscriber::set_default(subscriber(log.clone()));
    tracing::info!(
        request = 7_u64,
        method = "GET",
        route = "/pages/<name>",
        status = 200_u16,
        ms = 3_u64,
        "served"
    );
    tracing::info!(target: flare_server::METRICS_TARGET, "{}", r#"{"PagesViewed":1}"#);
    String::from_utf8(log.0.lock().unwrap().clone()).unwrap()
}

#[test]
fn a_metric_event_is_written_as_its_bare_json_line_and_every_other_line_as_before() {
    let before = written(|log| {
        Box::new(
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_target(false)
                .without_time()
                .with_writer(move || log.clone())
                .finish(),
        )
    });
    let after = written(|log| {
        let text = Format::default()
            .with_ansi(false)
            .with_target(false)
            .without_time();
        Box::new(
            tracing_subscriber::fmt()
                .with_ansi(false)
                .event_format(Lines::new(text))
                .with_writer(move || log.clone())
                .finish(),
        )
    });
    let before: Vec<&str> = before.lines().collect();
    let after: Vec<&str> = after.lines().collect();
    assert_eq!(after.len(), 2, "{after:?}");
    assert_eq!(after[0], before[0], "the served line is unchanged");
    assert!(before[0].contains("served"), "{before:?}");
    assert_eq!(after[1], r#"{"PagesViewed":1}"#);
}
