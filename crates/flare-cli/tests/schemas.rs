//! `flare schemas list` and `flare schemas show`: an agent reads the JSON
//! Schema of each document that `flare ... --json` prints from flare
//! itself, and each schema is the one that `tests/conventions.rs` pins in
//! `tests/golden/schemas`.
#![cfg(test)]

#[path = "../../flare-auth-loopback/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is flare-auth-loopback's test support, and these tests use part of it"
)]
pub mod loopback_scratch;
#[path = "support/run.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the run helper is test support, and each test file uses part of it"
)]
pub mod run;

use run::Home;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

/// The directory of the golden schema files, one for each document that
/// flare declares.
const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/schemas");

/// The documents that crumbkit, crumbkit-usage, crumbkit-skills and the
/// schema commands declare, which flare prints under its own name and crumbkit-test does
/// not pin in flare's golden files.
const CRUMBKIT_DOCUMENTS: &[&str] = &[
    "flare/failure/v1",
    "flare/commands-list/v1",
    "flare/self-usage/v1",
    "flare/schemas-list/v1",
    "flare/schemas-show/v1",
    "flare/skills-install/v1",
    "flare/skills-check/v1",
    "flare/skills-print/v1",
];

/// Each golden file's document id, such as `flare/shares/v1` for
/// `flare-shares-v1.json`, with the file's JSON.
fn golden_schemas() -> Vec<(String, Value)> {
    let mut schemas: Vec<(String, Value)> = std::fs::read_dir(GOLDEN)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ending| ending == "json"))
        .map(|path| (golden_id(&path), read_json(&path)))
        .collect();
    schemas.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!schemas.is_empty(), "no golden schemas in {GOLDEN}");
    schemas
}

/// The id of the golden file at `path`: `flare-<name>-v<N>.json` holds
/// `flare/<name>/v<N>`.
fn golden_id(path: &Path) -> String {
    let stem = path.file_stem().unwrap().to_str().unwrap();
    let rest = stem.strip_prefix("flare-").unwrap();
    let (name, version) = rest.rsplit_once('-').unwrap();
    format!("flare/{name}/{version}")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Runs `flare` with `args` in a scratch home and parses standard output
/// as JSON.
fn run_json(home: &Home, args: &[&str]) -> (Option<i32>, Value) {
    let ran = home.run(args);
    let json =
        serde_json::from_str(&ran.stdout).unwrap_or_else(|error| panic!("{error}: {}", ran.all()));
    (ran.code, json)
}

#[test]
fn schemas_list_lists_each_golden_document_and_crumbkit_s_documents_and_no_other() {
    let home = Home::new();
    let (code, listed) = run_json(&home, &["schemas", "list", "--all", "--json"]);
    assert_eq!(code, Some(0), "{listed}");
    let ids: Vec<&str> = listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "an id is listed twice: {ids:?}");
    let golden = golden_schemas();
    let mut expected: BTreeSet<&str> = golden.iter().map(|(id, _)| id.as_str()).collect();
    expected.extend(CRUMBKIT_DOCUMENTS);
    assert_eq!(unique, expected);
}

#[test]
fn schemas_show_prints_the_golden_schema_of_each_document_flare_declares() {
    let home = Home::new();
    let golden = golden_schemas();
    let mut args = vec!["schemas", "show"];
    args.extend(golden.iter().map(|(id, _)| id.as_str()));
    args.push("--json");
    let (code, shown) = run_json(&home, &args);
    assert_eq!(code, Some(0), "{shown}");
    let rows = shown["data"].as_array().unwrap();
    assert_eq!(rows.len(), golden.len(), "{shown}");
    for ((id, schema), row) in golden.iter().zip(rows) {
        assert_eq!(row["id"], id.as_str());
        assert_eq!(&row["schema"], schema, "{id} differs from its golden file");
    }
}

#[test]
fn schemas_show_fails_with_schemas_not_found_for_an_id_that_names_no_document() {
    let home = Home::new();
    let (code, shown) = run_json(&home, &["schemas", "show", "flare/nothing/v1", "--json"]);
    assert_eq!(code, Some(1), "{shown}");
    let failures = shown["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1, "{shown}");
    // crumbkit declares the code as `crumbkit::schemas::not_found` and prints
    // it under the tool's name.
    assert_eq!(failures[0]["code"], "flare::schemas::not_found");
    assert_eq!(failures[0]["recovery"]["kind"], "run");
    assert_eq!(failures[0]["recovery"]["call"], "flare schemas list");
}

#[test]
fn schemas_show_succeeds_for_an_id_that_names_a_document() {
    let home = Home::new();
    let (code, shown) = run_json(&home, &["schemas", "show", "flare/shares/v1", "--json"]);
    assert_eq!(code, Some(0), "{shown}");
    assert_eq!(shown["data"][0]["id"], "flare/shares/v1");
    assert!(
        shown["failures"].as_array().is_none_or(Vec::is_empty),
        "{shown}"
    );
}
