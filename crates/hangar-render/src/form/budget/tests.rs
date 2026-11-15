//! Unit tests of the post size count.

use super::*;
use crate::test_support::{crlf, urlencoded_len};

#[test]
fn the_encoded_size_matches_a_browsers_post_after_crlf_line_breaks() {
    for text in ["plain text", "a\nb\r\nc\rd", "\u{65e5}\u{1}\"&=+%", ""] {
        let posted = vec![("k".to_owned(), crlf(text))];
        assert_eq!(encoded(text) + 2, urlencoded_len(&posted), "{text:?}");
    }
}
