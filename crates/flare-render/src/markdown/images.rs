//! The images of a Markdown share: which relative paths a document shows,
//! and where the reading page loads each from.

use pulldown_cmark::{Event, Parser, Tag};
use std::collections::BTreeMap;

/// Where the reading page loads each image the version lists: its path as
/// the Markdown wrote it, and the URL of the file route. A relative image
/// path the map does not hold shows as its alternative text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageSources(BTreeMap<String, String>);

impl ImageSources {
    /// No listed images.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The sources that `pairs` of path and URL give.
    #[must_use]
    pub fn new(pairs: impl IntoIterator<Item = (String, String)>) -> Self {
        Self(pairs.into_iter().collect())
    }

    /// The URL for `path`, if the version lists it.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&str> {
        self.0.get(path).map(String::as_str)
    }
}

/// The relative image sources that `markdown` shows, in the order it shows
/// them, each once. A source that is not relative, such as an `https` URL
/// or a `data:` URL, is not listed, because the renderer never shows it.
#[must_use]
pub fn image_paths(markdown: &str) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for event in Parser::new_ext(markdown, super::OPTIONS) {
        if let Event::Start(Tag::Image { dest_url, .. }) = event
            && super::relative(&dest_url)
            && !paths.iter().any(|path| path.as_str() == dest_url.as_ref())
        {
            paths.push(dest_url.to_string());
        }
    }
    paths
}
