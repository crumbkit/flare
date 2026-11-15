//! Renders hangar's pages and turns the posts of its edit forms into record
//! writes.
//!
//! Every function is pure: the crate opens no socket, file or process, reads
//! no clock and keeps nothing between calls. The server hands it a
//! [`Page`](hangar_core::Page), the records that fill the page's slots, the
//! JSON Schema text of a record type and a [`FormTarget`], and it hands back
//! HTML or the body of a record write.
//!
//! - [`check_page`] is the publish-time check of a page's template, and
//!   [`render_page`] renders a page into a whole HTML document.
//! - [`edit_form`] draws the form that edits one filled slot.
//!   [`FormPost::from_pairs`] reads the form's post, [`FormPost::into_edit`]
//!   checks it against the slot and the schema, and [`Edit::apply`] turns it
//!   into a [`RecordWrite`], [`Outcome::Unchanged`] or a [`Conflict`].
//! - [`conflict_view`] draws a conflict and the form that saves the edit
//!   onto the newer record.
//! - [`stylesheet`] is the stylesheet that the markup's class names use.

mod apply;
mod body;
mod check;
mod conflict;
mod form;
mod html;
mod markdown;
mod post;
mod render;
mod schema;
#[cfg(test)]
mod test_support;
mod token;
mod values;

pub use apply::{ApplyError, Conflict, Outcome, RecordWrite};
pub use check::{Flaw, FlawKind, TemplateFlaws, check_page};
pub use conflict::conflict_view;
pub use form::{FormError, FormTarget, LocalPath, LocalPathError, Notice, edit_form};
pub use html::{Html, stylesheet};
pub use post::{Edit, FormPost, PostError};
pub use render::{PageDocument, RenderError, render_page};
pub use schema::{FieldKind, NumberRange, Schema, SchemaError};
pub use token::FormToken;
