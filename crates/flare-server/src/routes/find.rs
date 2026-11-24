//! Finding the route of a request in the table, and listing the table.

use super::{ROUTES, Route};
use crate::site::SiteKind;
use http::Method;

/// What the table has for a request.
#[derive(Debug)]
pub(crate) enum Found<'p> {
    /// This route, and the name segment when its pattern has one.
    Route(&'static Route, &'p str),
    /// A route has this path and another method.
    MethodNotAllowed,
    /// No route has this path.
    Nothing,
}

/// The text after a pattern's prefix when `path` matches it. A pattern
/// with text after `<name>` takes exactly one non-empty segment for the
/// name, so `/api/shares/<name>/versions` never matches a path with more
/// segments.
fn matches<'p>(pattern: &str, path: &'p str) -> Option<&'p str> {
    match pattern.split_once("<name>") {
        Some((prefix, "")) => path.strip_prefix(prefix),
        Some((prefix, suffix)) => path
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
            .filter(|name| !name.is_empty() && !name.contains('/')),
        None => (pattern == path).then_some(""),
    }
}

/// The route for `method` and `path` among the routes `site` serves.
pub(crate) fn find<'p>(method: &Method, path: &'p str, site: SiteKind) -> Found<'p> {
    let mut path_known = false;
    for route in ROUTES.iter().filter(|route| route.sites.serves(site)) {
        if let Some(name) = matches(route.pattern, path) {
            if route.method == method {
                return Found::Route(route, name);
            }
            path_known = true;
        }
    }
    if path_known {
        Found::MethodNotAllowed
    } else {
        Found::Nothing
    }
}

/// The method and pattern of every route the local site's launch host
/// serves.
pub(crate) fn listed() -> impl Iterator<Item = (Method, &'static str)> {
    ROUTES
        .iter()
        .filter(|route| route.sites.serves(SiteKind::Local))
        .map(|route| (route.method.clone(), route.pattern))
}
