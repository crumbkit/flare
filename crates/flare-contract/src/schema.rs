//! The JSON Schema of every request, response and problem type, which
//! `schema/flare-api.schema.json` commits.

use crate::{
    ActivityPage, ExchangeRequest, GrantRequest, OpenedShare, ProblemDocument, SearchAnswer,
    SearchFields, ShareList, ShareRequest, Shared, TokenIssued, VersionRequest,
};
use schemars::generate::SchemaSettings;
use std::fmt;

/// The command that writes the committed schema again.
const REGENERATE: &str =
    "cargo run -p flare-contract --example schema > schema/flare-api.schema.json";

/// The schema document: every type under `$defs`, as pretty JSON with a
/// final newline.
#[must_use]
pub fn generate() -> String {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    generator.subschema_for::<ShareRequest>();
    generator.subschema_for::<VersionRequest>();
    generator.subschema_for::<GrantRequest>();
    generator.subschema_for::<Shared>();
    generator.subschema_for::<ShareList>();
    generator.subschema_for::<OpenedShare>();
    generator.subschema_for::<ActivityPage>();
    generator.subschema_for::<ExchangeRequest>();
    generator.subschema_for::<TokenIssued>();
    generator.subschema_for::<SearchAnswer>();
    generator.subschema_for::<SearchFields>();
    generator.subschema_for::<ProblemDocument>();
    let defs = generator.take_definitions(true);
    let document = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "flare API",
        "$defs": defs,
    });
    let mut text = serde_json::to_string_pretty(&document).unwrap_or_default();
    text.push('\n');
    text
}

/// The committed schema differs from the contract's types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    line: usize,
}

impl fmt::Display for Drift {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "schema/flare-api.schema.json differs from the contract's types from line {}; run `{REGENERATE}` and commit the result",
            self.line
        )
    }
}

impl std::error::Error for Drift {}

/// Compares the committed schema with the generated one.
///
/// # Errors
///
/// Returns [`Drift`], with the first line that differs, when they differ.
pub fn check(committed: &str) -> Result<(), Drift> {
    let generated = generate();
    if generated == committed {
        return Ok(());
    }
    let line = generated
        .lines()
        .zip(committed.lines())
        .take_while(|(one, two)| one == two)
        .count()
        .saturating_add(1);
    Err(Drift { line })
}
