//! The reading page of a Markdown share at `/s/<id>`, in the signed Flare
//! mockup's layout (artboard 01): a header with the flare name and the
//! reader's initials, then the article with a line naming who shared it,
//! with whom, the version and the date, the title, and the rendered
//! Markdown. The page has no Inbox, Shared by me or Search link and no
//! comment or response count; the changes that build those add them.
//!
//! Below the article, the foot says the page was shared with Flare. For a
//! reader who has never shared a flare on a site that sets a get flare
//! address, the foot also holds the Get Flare link of the signed badge
//! mockup (option a), which opens that address in a new tab.

use crate::docs::escape;
use crate::sharing::Reading;
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

/// The line under the header: who shared the share, with whom, the version
/// and the date.
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
    format!(
        "<div class=\"share-meta\"><div class=\"line\"><span class=\"who\" aria-hidden=\"true\">{initials}</span><span>Shared by {owner} with {with}</span><span>\u{b7} version {version} \u{b7} <time datetime=\"{stamp}\">{shown}</time></span></div></div>\n",
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

/// The whole reading page for `reader`. `script` is the entry script's file
/// name; the page loads it only when the article has code or a diagram.
pub(crate) fn document(
    reading: &Reading,
    reader: &str,
    article: &Article,
    script: Option<&str>,
) -> String {
    let script = match script {
        Some(file) if article.needs_script() => format!(
            "<script type=\"module\" src=\"/flare/assets/{}\"></script>\n",
            escape(file)
        ),
        _ => String::new(),
    };
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{title} - flare</title>\n<link rel=\"stylesheet\" href=\"/flare/style.css\">\n{script}</head>\n<body class=\"flare reading\">\n<header class=\"bar\"><span class=\"brand\">flare</span><span class=\"who\" title=\"{reader}\"><span class=\"visually-hidden\">Signed in as {reader}: </span>{reader_initials}</span></header>\n<main class=\"screen\">\n<article class=\"doc\">\n{meta}<h1>{title}</h1>\n{body}</article>\n</main>\n{foot}</body>\n</html>\n",
        title = escape(&reading.title),
        reader = escape(reader),
        reader_initials = escape(&initials(reader)),
        meta = meta_line(reading),
        body = article.html().as_str(),
        foot = foot(reading),
    )
}

#[cfg(test)]
mod tests;
