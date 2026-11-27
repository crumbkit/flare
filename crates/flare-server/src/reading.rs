//! The reading page of a Markdown share at `/s/<id>`, in the signed Flare
//! mockup's layout (artboard 01): a header with the flare name and the
//! reader's initials, then the article with a line naming who shared it,
//! with whom, the version and the date, the comment count linked to the
//! comments, the title, and the rendered Markdown. The page has no Inbox,
//! Shared by me or Search link and no response count; the changes that
//! build those add them.
//!
//! Below the article, the comments section of artboard 03 holds only its
//! heading and "Loading comments". The comments entry script, which every
//! page loads, reads the thread and draws the section in its place. When
//! that script never runs, a line saying the comments did not load shows
//! after ten seconds (the `.late` rule in docs.rs).
//!
//! Below the article, the foot says the page was shared with Flare. For a
//! reader who has never shared a flare on a site that sets a get flare
//! address, the foot also holds the Get Flare link of the signed badge
//! mockup (option a), which opens that address in a new tab.

use crate::docs::escape;
use crate::sharing::{Count, Reading};
use flare_render::Article;

/// The months, as the date on the page names them.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Up to two capital letters for `address`: the first letters of the first
/// two words of its local part, or the first two letters of a one-word local
/// part.
pub(crate) fn initials(address: &str) -> String {
    let local = address.split('@').next().unwrap_or_default();
    let words: Vec<&str> = local
        .split(['.', '_', '-', '+'])
        .filter(|word| !word.is_empty())
        .collect();
    let letters: String = match words.as_slice() {
        [first, second, ..] => first
            .chars()
            .take(1)
            .chain(second.chars().take(1))
            .collect(),
        [only] => only.chars().take(2).collect(),
        [] => String::from("?"),
    };
    letters.to_uppercase()
}

/// The date of `seconds` since 1970 as the page shows it, such as
/// "21 Nov 2026", and as an RFC 3339 time for the `<time>` element.
pub(crate) fn date(seconds: u64) -> (String, String) {
    let stamp = flare_contract::utc(seconds);
    let mut parts = stamp.get(..10).unwrap_or_default().split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return (stamp.clone(), stamp);
    };
    let month = month
        .parse::<usize>()
        .ok()
        .and_then(|month| MONTHS.get(month.wrapping_sub(1)))
        .copied()
        .unwrap_or(month);
    let day = day.trim_start_matches('0');
    (format!("{day} {month} {year}"), stamp)
}

/// The comment count as the line under the header shows it, in the words
/// of the comments section's heading, such as "2 comments, 1 reply".
fn count(count: Count) -> String {
    let plural = |number: u32, one: &str, many: &str| {
        format!("{number} {}", if number == 1 { one } else { many })
    };
    if count.comments == 0 {
        return String::from("No comments");
    }
    let comments = plural(count.comments, "comment", "comments");
    if count.replies == 0 {
        comments
    } else {
        format!("{comments}, {}", plural(count.replies, "reply", "replies"))
    }
}

/// The line under the header: who shared the share, with whom, the version
/// and the date, then the comment count linked to the comments section.
fn meta_line(reading: &Reading) -> String {
    let owner = if reading.own {
        String::from("you")
    } else {
        format!("<b>{}</b>", escape(&reading.owner))
    };
    let with = match &reading.with {
        None => String::from("you"),
        Some(grantees) if grantees.is_empty() => String::from("no one yet"),
        Some(grantees) => grantees
            .iter()
            .map(|grantee| format!("<span class=\"mono\">{}</span>", escape(grantee)))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let (shown, stamp) = date(reading.published);
    let chips = reading.comments.map_or_else(String::new, |comments| {
        format!(
            "<div class=\"row\"><a class=\"chip\" href=\"#comments\">{}</a></div>",
            count(comments)
        )
    });
    format!(
        "<div class=\"share-meta\"><div class=\"line\"><span class=\"who\" aria-hidden=\"true\">{initials}</span><span>Shared by {owner} with {with}</span><span>\u{b7} version {version} \u{b7} <time datetime=\"{stamp}\">{shown}</time></span></div>{chips}</div>\n",
        initials = escape(&initials(&reading.owner)),
        version = reading.version,
        stamp = escape(&stamp),
        shown = escape(&shown),
    )
}

/// The flame mark of the Get Flare link, as the badge mockup draws it.
const FLAME: &str = "<svg viewBox=\"0 0 16 16\" aria-hidden=\"true\"><path d=\"M8.6 0.8c.3 2.2-.7 3.3-1.8 4.5C5.7 6.5 4.5 7.8 4.5 9.9c0 2.2 1.6 3.8 3.5 3.8s3.5-1.5 3.5-3.6c0-1.2-.5-2.1-1-2.9-.3 1-.9 1.5-1.6 1.7.4-2.6.2-5.4-.3-8.1z\"/></svg>";

/// The page's foot: the note, and the Get Flare link when the reading
/// carries an address.
fn foot(reading: &Reading) -> String {
    let link = reading.badge.as_deref().map_or_else(String::new, |address| {
        format!(
            "<a class=\"badge\" href=\"{href}\" target=\"_blank\" rel=\"noopener noreferrer\" aria-label=\"Get Flare, opens in a new tab\">{FLAME}Get Flare<span class=\"arr\" aria-hidden=\"true\">\u{2197}</span></a>",
            href = escape(address),
        )
    });
    format!(
        "<footer class=\"share-foot\"><span class=\"note\">Shared with Flare</span>{link}</footer>\n"
    )
}

/// The comments section below the article. The comments entry script
/// reads the share, the version and the owner from its data attributes.
fn comments(reading: &Reading) -> String {
    format!(
        "<section id=\"comments\" class=\"comments\" aria-labelledby=\"comments-heading\" data-flare=\"{id}\" data-version=\"{version}\" data-owner=\"{owner}\"><div class=\"thread\"><div class=\"section-head\"><h2 id=\"comments-heading\">Comments</h2><span class=\"status\">Loading comments</span></div><p class=\"late\">The comments did not load. Reload the page to try again.</p></div></section>\n",
        id = escape(&reading.id),
        version = reading.version,
        owner = escape(&reading.owner),
    )
}

/// The file names of the embedded entry scripts, each absent when the
/// build embedded no bundle.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Entries<'a> {
    /// The script that colours code and shows its Copy buttons.
    pub(crate) reading: Option<&'a str>,
    /// The script that draws the comments section.
    pub(crate) comments: Option<&'a str>,
}

/// The entry scripts the page loads: the comments entry always, and the
/// reading entry only when the article has code, so a page without code
/// never fetches Shiki. The server draws diagrams, so they need no script.
fn scripts<'a>(article: &Article, entries: Entries<'a>) -> Vec<&'a str> {
    let reading = entries.reading.filter(|_| article.needs_script());
    reading.into_iter().chain(entries.comments).collect()
}

/// Whether the page loads any script, so its reply needs the script policy.
pub(crate) fn loads_script(article: &Article, entries: Entries<'_>) -> bool {
    !scripts(article, entries).is_empty()
}

/// The whole reading page for `reader`, loading the scripts of `entries`
/// that it needs.
pub(crate) fn document(
    reading: &Reading,
    reader: &str,
    article: &Article,
    entries: Entries<'_>,
) -> String {
    page(
        reading,
        reader,
        article.html().as_str(),
        &scripts(article, entries),
    )
}

/// The whole reading page of an uploaded HTML share: `frame` takes the
/// article's place, and only the comments entry loads.
pub(crate) fn framed_document(
    reading: &Reading,
    reader: &str,
    frame: &str,
    entries: Entries<'_>,
) -> String {
    let scripts: Vec<&str> = entries.comments.into_iter().collect();
    page(reading, reader, frame, &scripts)
}

/// The page around `body`, loading the entry scripts in `files`.
fn page(reading: &Reading, reader: &str, body: &str, files: &[&str]) -> String {
    let script: String = files
        .iter()
        .map(|file| {
            format!(
                "<script type=\"module\" src=\"/flare/assets/{}\"></script>\n",
                escape(file)
            )
        })
        .collect();
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{title} - flare</title>\n<link rel=\"stylesheet\" href=\"/flare/style.css\">\n{script}</head>\n<body class=\"flare reading\">\n<header class=\"bar\"><span class=\"brand\">flare</span><span class=\"who\" title=\"{reader}\"><span class=\"visually-hidden\">Signed in as {reader}: </span>{reader_initials}</span></header>\n<main class=\"screen\">\n<article class=\"doc\">\n{meta}<h1>{title}</h1>\n{body}</article>\n{comments}</main>\n{foot}</body>\n</html>\n",
        title = escape(&reading.title),
        reader = escape(reader),
        reader_initials = escape(&initials(reader)),
        meta = meta_line(reading),
        comments = comments(reading),
        foot = foot(reading),
    )
}

#[cfg(test)]
mod tests;
