use super::{Anchor, MAX_CONTEXT_CHARS, MAX_QUOTE_CHARS, Quote, QuoteContext};
use crate::error::{ParseError, Subject};
use crate::share::ShareVersion;

#[test]
fn a_quote_holds_3_to_500_characters_counted_as_characters() {
    assert_eq!(Quote::try_from("abc").unwrap().as_str(), "abc");
    let longest = "é".repeat(MAX_QUOTE_CHARS);
    assert_eq!(Quote::try_from(longest.as_str()).unwrap().as_str(), longest);
    assert_eq!(
        Quote::try_from("é".repeat(MAX_QUOTE_CHARS + 1).as_str()),
        Err(ParseError::TooLong {
            subject: Subject::AnchorQuote,
            max: MAX_QUOTE_CHARS,
        })
    );
}

#[test]
fn an_empty_quote_and_a_quote_under_3_characters_are_refused() {
    assert_eq!(
        Quote::try_from(""),
        Err(ParseError::Empty {
            subject: Subject::AnchorQuote
        })
    );
    assert!(matches!(
        Quote::try_from("ab"),
        Err(ParseError::Malformed {
            subject: Subject::AnchorQuote,
            ..
        })
    ));
}

#[test]
fn a_quote_or_its_context_with_a_control_character_is_refused() {
    assert!(matches!(
        Quote::try_from("a\nbc"),
        Err(ParseError::Malformed {
            subject: Subject::AnchorQuote,
            ..
        })
    ));
    assert!(matches!(
        QuoteContext::try_from("a\tb"),
        Err(ParseError::Malformed {
            subject: Subject::AnchorContext,
            ..
        })
    ));
}

#[test]
fn a_context_holds_0_to_64_characters() {
    assert_eq!(QuoteContext::try_from("").unwrap().as_str(), "");
    let longest = "x".repeat(MAX_CONTEXT_CHARS);
    assert_eq!(
        QuoteContext::try_from(longest.as_str()).unwrap().as_str(),
        longest
    );
    assert_eq!(
        QuoteContext::try_from("x".repeat(MAX_CONTEXT_CHARS + 1).as_str()),
        Err(ParseError::TooLong {
            subject: Subject::AnchorContext,
            max: MAX_CONTEXT_CHARS,
        })
    );
}

#[test]
fn an_anchor_keeps_its_quote_context_and_version() {
    let anchor = Anchor::new(
        Quote::try_from("the sailing is held").unwrap(),
        QuoteContext::try_from("A new forecast arrives every hour. ").unwrap(),
        QuoteContext::try_from(" and passengers").unwrap(),
        ShareVersion::try_from(2).unwrap(),
    );
    assert_eq!(anchor.quote().as_str(), "the sailing is held");
    assert_eq!(
        anchor.prefix().as_str(),
        "A new forecast arrives every hour. "
    );
    assert_eq!(anchor.suffix().as_str(), " and passengers");
    assert_eq!(anchor.version().get(), 2);
}

#[test]
fn the_debug_output_of_an_anchor_gives_lengths_and_no_page_text() {
    let anchor = Anchor::new(
        Quote::try_from("secret words").unwrap(),
        QuoteContext::try_from("before").unwrap(),
        QuoteContext::try_from("after").unwrap(),
        ShareVersion::first(),
    );
    let shown = format!("{anchor:?}");
    assert!(!shown.contains("secret"), "{shown}");
    assert!(!shown.contains("before"), "{shown}");
    assert!(shown.contains("12 chars"), "{shown}");
}
