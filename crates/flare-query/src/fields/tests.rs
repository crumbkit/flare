use super::{FIELDS, FieldKind, ShareType, check, unknown_person};
use crate::error::ErrorCode;
use crate::parse;
use crate::query::{Field, Span, Term};

#[test]
fn the_field_list_has_type_with_the_share_formats_and_from_with_me() {
    assert_eq!(FIELDS.len(), 2);
    assert_eq!(FIELDS[0].field, Field::Type);
    assert_eq!(FIELDS[0].kind, FieldKind::Keyword);
    assert_eq!(FIELDS[0].values, ["markdown", "html"]);
    assert_eq!(FIELDS[1].field, Field::From);
    assert_eq!(FIELDS[1].kind, FieldKind::Person);
    assert_eq!(FIELDS[1].values, ["me"]);
}

#[test]
fn a_type_value_names_a_share_format_in_any_case() {
    assert_eq!(ShareType::named("markdown"), Some(ShareType::Markdown));
    assert_eq!(ShareType::named("HTML"), Some(ShareType::Html));
    assert_eq!(ShareType::named("pdf"), None);
}

#[test]
fn check_finds_each_type_value_that_names_no_format_and_passes_the_rest() {
    let query = parse("type:markdown,pdf -type:Html from:anyone type:doc").unwrap();
    let errors = check(&query);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0].code, ErrorCode::UnknownType);
    assert_eq!(errors[0].span, Span::new(14, 17));
    assert_eq!(
        errors[0].message,
        "There is no type called pdf; use type:markdown or type:html."
    );
    assert_eq!(errors[1].span, Span::new(46, 49));
    assert!(check(&parse("type:markdown -type:HTML from:x").unwrap()).is_empty());
}

#[test]
fn an_unknown_person_error_names_the_value_and_its_span() {
    let query = parse("from:nobody").unwrap();
    let Term::Filter { values, .. } = &query.parts()[0].term else {
        panic!("{query:?}");
    };
    let error = unknown_person(&values[0]);
    assert_eq!(error.code, ErrorCode::UnknownPerson);
    assert_eq!(error.span, Span::new(5, 11));
    assert_eq!(
        error.message,
        "Nobody called nobody is known here; use me, an email address or a name this server knows."
    );
}
