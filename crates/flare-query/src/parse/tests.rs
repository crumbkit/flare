use super::{MAX_CHARS, parse};
use crate::error::ErrorCode;
use crate::query::{Field, Span, Term};

fn message(text: &str) -> String {
    let errors = parse(text).unwrap_err();
    errors.first().unwrap().message.clone()
}

#[test]
fn a_mistyped_filter_name_suggests_the_filter_it_is_closest_to() {
    assert_eq!(
        message("form:erin"),
        "There is no filter called form; did you mean from?"
    );
    assert_eq!(
        message("tpye:html"),
        "There is no filter called tpye; did you mean type?"
    );
}

#[test]
fn a_filter_of_a_later_increment_names_the_filters_this_version_has() {
    assert_eq!(
        message("updated:7d"),
        "There is no filter called updated yet; use type: or from:, or put the text in quotes to search for it."
    );
}

#[test]
fn an_unclosed_quote_says_to_close_it() {
    assert_eq!(
        message("\"budget review"),
        "This quote is not closed; add \" after the last word of the phrase."
    );
}

#[test]
fn an_empty_from_value_names_the_kinds_of_person_it_takes() {
    assert_eq!(
        message("from:"),
        "from needs a person, such as from:me or from:ana@example.org."
    );
    assert_eq!(
        message("type:"),
        "type needs a kind of share, such as type:markdown."
    );
}

#[test]
fn a_query_of_the_most_characters_parses_and_one_more_is_too_long() {
    let longest = "a".repeat(MAX_CHARS);
    assert!(parse(&longest).is_ok());
    let errors = parse(&format!("{longest}b")).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::TooLong);
    assert_eq!(errors[0].span, Span::new(MAX_CHARS, MAX_CHARS + 1));
}

#[test]
fn an_unknown_filter_with_a_quoted_value_gives_one_error_and_no_stray_quote() {
    let errors = parse("foo:\"a b\" budget").unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, ErrorCode::UnknownFilter);
    assert_eq!(errors[0].span, Span::new(0, 3));
}

#[test]
fn a_tab_or_a_newline_separates_parts_as_a_space_does() {
    let query = parse("budget\treview\nfrom:me").unwrap();
    assert_eq!(query.parts().len(), 3);
    assert_eq!(query.to_string(), "budget review from:me");
    let Term::Filter { field, .. } = &query.parts()[2].term else {
        panic!("{query:?}");
    };
    assert_eq!(*field, Field::From);
}
