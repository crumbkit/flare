use super::{SearchError, SearchPart, SearchSpan};
use flare_query::parse;

#[test]
fn a_parsed_part_serialises_as_the_case_file_writes_it() {
    let query = parse("-from:ana,\"jo b\"").unwrap();
    let part = SearchPart::from(&query.parts()[0]);
    let value = serde_json::to_value(&part).unwrap();
    let case = serde_json::to_value(&query.parts()[0]).unwrap();
    assert_eq!(value, case);
    assert_eq!(value["values"][1]["span"]["end"], 16);
}

#[test]
fn a_query_error_keeps_its_code_span_and_sentence() {
    let errors = parse("form:x").unwrap_err();
    let error = SearchError::from(&errors[0]);
    assert_eq!(error.code, "flare::search::unknown_filter");
    assert_eq!(error.span, SearchSpan { start: 0, end: 4 });
    assert!(error.message.starts_with("There is no filter called form"));
}
