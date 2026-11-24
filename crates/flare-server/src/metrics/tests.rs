use super::{Event, METRICS_TARGET, Metric, NAMESPACE, Reason, StatusClass, Totals, line};
use flare_auth_oidc::SignInRefusal;
use serde_json::Value;
use std::collections::BTreeSet;

/// 2026-11-23T09:00:00Z in milliseconds.
const MORNING: u64 = 1_795_424_400_000;
/// 2026-11-24T09:00:00Z in milliseconds.
const NEXT_MORNING: u64 = MORNING + 86_400_000;

/// One event of each kind.
fn every_event() -> Vec<Event> {
    vec![
        Event::Request {
            route: "/pages/<name>",
            status: StatusClass::of(200),
        },
        Event::ShareCreated,
        Event::PageViewed,
        Event::SignedIn,
        Event::SignInRefused {
            reason: Reason::of(SignInRefusal::NotAllowed),
        },
        Event::TokenExchanged,
        Event::TokenRefused,
    ]
}

#[test]
fn each_event_writes_one_line_that_parses_as_emf_with_only_its_permitted_dimensions() {
    for event in every_event() {
        let text = line(event, MORNING);
        assert!(!text.contains('\n'), "{text}");
        let value: Value = serde_json::from_str(&text).unwrap();
        let aws = &value["_aws"];
        assert_eq!(aws["Timestamp"], MORNING, "{text}");
        let directives = aws["CloudWatchMetrics"].as_array().unwrap();
        assert_eq!(directives.len(), 1, "{text}");
        let directive = &directives[0];
        assert_eq!(directive["Namespace"], NAMESPACE, "{text}");
        let metrics = directive["Metrics"].as_array().unwrap();
        assert_eq!(metrics.len(), 1, "{text}");
        let name = metrics[0]["Name"].as_str().unwrap();
        assert_eq!(name, event.metric().name(), "{text}");
        assert_eq!(metrics[0]["Unit"], "Count", "{text}");
        assert_eq!(value[name], 1, "{text}");
        let sets = directive["Dimensions"].as_array().unwrap();
        assert_eq!(sets.len(), 1, "{text}");
        let keys: Vec<&str> = sets[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap())
            .collect();
        assert_eq!(keys, event.metric().dimensions(), "{text}");
        let mut expected: BTreeSet<&str> = keys.iter().copied().collect();
        expected.extend(["_aws", name]);
        let present: BTreeSet<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(present, expected, "the line holds another key: {text}");
        for key in keys {
            assert!(value[key].is_string(), "{text}");
        }
    }
}

#[test]
fn a_request_line_holds_the_route_pattern_and_the_status_class() {
    let text = line(
        Event::Request {
            route: "/api/shares/<name>",
            status: StatusClass::of(404),
        },
        MORNING,
    );
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["Route"], "/api/shares/<name>");
    assert_eq!(value["StatusClass"], "4xx");
}

#[test]
fn a_refused_sign_in_line_holds_the_reason() {
    let text = line(
        Event::SignInRefused {
            reason: Reason::of(SignInRefusal::NoKeptState),
        },
        MORNING,
    );
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["Reason"], "no_kept_state");
}

#[test]
fn each_status_names_its_class() {
    for (status, class) in [
        (200, "2xx"),
        (201, "2xx"),
        (204, "2xx"),
        (303, "3xx"),
        (400, "4xx"),
        (401, "4xx"),
        (404, "4xx"),
        (500, "5xx"),
        (503, "5xx"),
    ] {
        assert_eq!(StatusClass::of(status).text(), class, "{status}");
    }
}

#[test]
fn every_refusal_of_flare_auth_oidc_has_its_own_reason() {
    let refusals = [
        SignInRefusal::Malformed,
        SignInRefusal::NoKeptState,
        SignInRefusal::WrongState,
        SignInRefusal::UnknownState,
        SignInRefusal::ProviderDenied,
        SignInRefusal::CodeRejected,
        SignInRefusal::ProviderUnavailable,
        SignInRefusal::ProviderMisbehaved,
        SignInRefusal::TokenRejected(flare_auth_oidc::TokenFault::Expired),
        SignInRefusal::EmailUnverified,
        SignInRefusal::NotAllowed,
        SignInRefusal::TooManySessions,
        SignInRefusal::NoRandomness,
        SignInRefusal::CannotSeal,
        SignInRefusal::NoHostedDomain,
        SignInRefusal::OtherHostedDomain,
    ];
    let reasons: BTreeSet<&str> = refusals
        .iter()
        .map(|refusal| Reason::of(*refusal).text())
        .collect();
    assert_eq!(reasons.len(), refusals.len(), "{reasons:?}");
    assert!(!reasons.contains("other"), "{reasons:?}");
}

/// The rows of the first Markdown table after `heading` in `docs`, as
/// cells with their backticks removed.
fn table_after(docs: &str, heading: &str) -> Vec<Vec<String>> {
    let start = docs.find(heading).unwrap();
    docs[start..]
        .lines()
        .skip_while(|line| !line.starts_with('|'))
        .take_while(|line| line.starts_with('|'))
        .skip(2)
        .map(|line| {
            line.trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().replace('`', ""))
                .collect()
        })
        .collect()
}

fn docs() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/metrics.md");
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn the_metric_names_and_dimensions_equal_the_list_in_docs_metrics_md() {
    let docs = docs();
    let listed: Vec<(String, Vec<String>)> = table_after(&docs, "## The metrics")
        .into_iter()
        .map(|cells| {
            let dimensions = if cells[1] == "none" {
                Vec::new()
            } else {
                cells[1].split(", ").map(str::to_owned).collect()
            };
            (cells[0].clone(), dimensions)
        })
        .collect();
    let code: Vec<(String, Vec<String>)> = Metric::ALL
        .iter()
        .map(|metric| {
            (
                metric.name().to_owned(),
                metric
                    .dimensions()
                    .iter()
                    .map(|key| (*key).to_owned())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(listed, code);
}

#[test]
fn the_refusal_reasons_equal_the_list_in_docs_metrics_md() {
    let docs = docs();
    let listed: Vec<String> = table_after(&docs, "| Reason | What happened |")
        .into_iter()
        .map(|cells| cells[0].clone())
        .collect();
    let code: Vec<String> = Reason::ALL
        .iter()
        .map(|reason| reason.text().to_owned())
        .collect();
    assert_eq!(listed, code);
}

#[test]
fn the_metrics_target_is_flare_metrics() {
    assert_eq!(METRICS_TARGET, "flare::metrics");
}

#[test]
fn totals_count_each_metric_on_each_day_in_utc() {
    let mut totals = Totals::default();
    for (event, at) in [
        (Event::PageViewed, MORNING),
        (Event::PageViewed, MORNING),
        (Event::SignedIn, MORNING),
        (Event::PageViewed, NEXT_MORNING),
    ] {
        totals.add(&line(event, at));
    }
    let days: Vec<(&str, &str, u64)> = totals.by_day().collect();
    assert_eq!(
        days,
        [
            ("2026-11-23", "PagesViewed", 2),
            ("2026-11-23", "SignIns", 1),
            ("2026-11-24", "PagesViewed", 1),
        ]
    );
    let all: Vec<(&str, u64)> = totals.by_metric().collect();
    assert_eq!(all, [("PagesViewed", 3), ("SignIns", 1)]);
    assert_eq!(totals.lines(), 4);
    assert_eq!(totals.skipped(), 0);
}

#[test]
fn totals_skip_and_count_every_line_that_is_not_a_flare_emf_line() {
    let good = line(Event::ShareCreated, MORNING);
    let other = good.replace("\"Flare\"", "\"Other\"");
    let mut totals = Totals::default();
    for text in [
        "2026-11-23T09:00:00.000000Z  INFO served request=1 method=GET route=/up status=200 ms=0",
        "",
        "{\"_aws\":{\"Timestamp\":1795424400000,",
        "[1, 2]",
        "{\"no\":\"aws\"}",
        &other,
        &good,
    ] {
        totals.add(text);
    }
    assert_eq!(totals.lines(), 7);
    assert_eq!(totals.skipped(), 6);
    let all: Vec<(&str, u64)> = totals.by_metric().collect();
    assert_eq!(all, [("SharesCreated", 1)]);
}

#[test]
fn the_example_lines_in_docs_metrics_md_are_the_lines_the_server_writes() {
    let docs = docs();
    for event in [
        Event::PageViewed,
        Event::Request {
            route: "/pages/<name>",
            status: StatusClass::of(200),
        },
    ] {
        let text = line(event, MORNING);
        assert!(docs.contains(&text), "docs/metrics.md lacks {text}");
    }
}
