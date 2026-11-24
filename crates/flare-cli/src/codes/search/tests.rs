use super::{Refused, Searched};
use flare_contract::{SearchError, SearchSpan};

fn error(start: u32, end: u32, message: &str) -> SearchError {
    SearchError {
        code: "flare::search::unknown_field".to_owned(),
        span: SearchSpan { start, end },
        message: message.to_owned(),
    }
}

#[test]
fn the_causes_are_the_query_and_a_caret_line_under_each_error() {
    let searched = Searched {
        query: Some("budget form:x \"".to_owned()),
        errors: vec![error(7, 11, "No form."), error(14, 15, "Close it.")],
    };
    assert_eq!(
        searched.causes(),
        [
            "budget form:x \"",
            "       ^^^^ No form.",
            "              ^ Close it.",
        ]
    );
}

#[test]
fn without_the_query_each_error_names_its_characters() {
    let searched = Searched {
        query: None,
        errors: vec![error(0, 4, "No form.")],
    };
    assert_eq!(searched.causes(), ["characters 0 to 4: No form."]);
    assert!(Searched::default().causes().is_empty());
}

#[test]
fn an_invalid_search_refusal_keeps_the_query_and_every_error() {
    let errors = flare_query::parse("form:x -").unwrap_err();
    let refused = Refused::invalid_search("form:x -", &errors);
    assert_eq!(refused.searched.causes().len(), 3);
    assert_eq!(refused.api_code(), flare_contract::ApiCode::SearchInvalid);
}

#[test]
fn an_invalid_search_refusal_names_its_query_and_errors_as_fields() {
    use crumbkit::{Failure, FieldValue};
    let errors = flare_query::parse("form:x").unwrap_err();
    let refused = Refused::invalid_search("form:x", &errors);
    let fields = refused.fields();
    let names: Vec<&str> = fields.iter().map(|field| field.name()).collect();
    assert_eq!(names, ["query", "errors"]);
    assert_eq!(fields[0].value(), &FieldValue::from("form:x"));
    let FieldValue::List(listed) = fields[1].value() else {
        panic!("errors is not a list: {:?}", fields[1].value());
    };
    assert_eq!(
        listed,
        &[FieldValue::object([
            ("code", FieldValue::from("flare::search::unknown_field")),
            ("start", FieldValue::from(0_u32)),
            ("end", FieldValue::from(4_u32)),
            ("message", FieldValue::from(errors[0].message.as_str())),
        ])]
    );
}

#[test]
fn a_refusal_of_another_code_has_no_search_fields() {
    use crumbkit::Failure;
    let refused = Refused::new(
        flare_contract::ApiCode::SearchInvalid,
        None,
        "https://example.test",
    );
    let names: Vec<&str> = refused.fields().iter().map(|field| field.name()).collect();
    assert_eq!(names, ["server"]);
}
