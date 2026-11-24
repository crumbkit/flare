//! flare's search syntax: [`parse`] turns the text a person types into a
//! [`Query`] of [`Part`]s, and a query's `Display` prints its canonical
//! text.
//!
//! The first slice of the syntax has plain words, quoted phrases, the
//! `type:` and `from:` filters, a comma inside a filter's value meaning any
//! of, and a leading minus that excludes a part. A space between parts
//! means all of them must match. Any other `field:` is an unknown filter
//! until the increment that adds the field lands.
//!
//! A text with an error gives every error, each with a stable code, the
//! span it covers and one sentence that says how to fix it, and no query,
//! so nothing runs for it. Spans count Unicode scalar values from 0, with
//! an exclusive end, so a parser in another language counts them alike.
//!
//! `cases/search-cases.json` holds the inputs that every parser of the
//! syntax must agree on: each input's parts and canonical text, or its
//! error codes and spans. Board copies the file for its own parser.
//! [`fields`] holds flare's own filter list and the check of `type:`
//! values, which Board does not share.

mod error;
pub mod fields;
mod parse;
mod query;

pub use error::{ErrorCode, QueryError};
pub use parse::{MAX_CHARS, parse};
pub use query::{Field, Part, Query, Span, Term, Value};
