//! The reading page's markup, against artboard 01 of the signed mockup
//! with the agreed differences: no Inbox, Shared by me or Search link and
//! no comment or response count, and the foot of the get flare badge
//! mockup.

use super::*;

fn reading() -> Reading {
    Reading {
        title: String::from("Weather holds for the island ferry"),
        owner: String::from("ana.okafor@example.com"),
        own: false,
        with: None,
        version: 3,
        published: 1_795_219_200,
        markdown: String::from("A sailing is held.\n"),
        badge: None,
    }
}

fn page(reading: &Reading, markdown: &str, script: Option<&str>) -> String {
    let article = flare_render::article(markdown).unwrap();
    document(reading, "priya.nair@example.com", &article, script)
}

#[test]
fn the_header_holds_the_flare_name_and_the_readers_initials_and_no_other_link() {
    let html = page(&reading(), "Text.\n", None);
    let header = html
        .split_once("<header class=\"bar\">")
        .and_then(|(_, rest)| rest.split_once("</header>"))
        .map(|(header, _)| header)
        .unwrap();
    assert!(
        header.contains("<span class=\"brand\">flare</span>"),
        "{header}"
    );
    assert!(header.contains(">PN</span>"), "{header}");
    assert!(!header.contains("<a "), "{header}");
    assert!(!header.contains("<nav"), "{header}");
    for link in ["Inbox", "Shared by me", "Search"] {
        assert!(!html.contains(link), "{link}");
    }
}

#[test]
fn the_line_under_the_header_names_who_shared_it_with_whom_the_version_and_the_date() {
    let html = page(&reading(), "Text.\n", None);
    assert!(
        html.contains("Shared by <b>ana.okafor@example.com</b> with you"),
        "{html}"
    );
    assert!(html.contains("version 3"), "{html}");
    assert!(html.contains(">21 Nov 2026</time>"), "{html}");
    assert!(html.contains(">AO</span>"), "{html}");
    assert!(!html.contains("comment"), "{html}");
    assert!(!html.contains("response"), "{html}");
    let line = html.find("class=\"share-meta\"").unwrap();
    let title = html
        .find("<h1>Weather holds for the island ferry</h1>")
        .unwrap();
    assert!(line < title);
}

#[test]
fn the_owner_sees_the_people_the_share_is_granted_to() {
    let mut owned = reading();
    owned.own = true;
    owned.with = Some(vec![
        String::from("bob@example.com"),
        String::from("carol@example.com"),
    ]);
    let html = page(&owned, "Text.\n", None);
    assert!(html.contains("Shared by you with <span class=\"mono\">bob@example.com</span>, <span class=\"mono\">carol@example.com</span>"), "{html}");
    owned.with = Some(Vec::new());
    assert!(page(&owned, "Text.\n", None).contains("Shared by you with no one yet"));
}

#[test]
fn the_title_owner_and_grantees_are_escaped() {
    let mut hostile = reading();
    hostile.title = String::from("<script>alert(1)</script>");
    hostile.owner = String::from("\"><img src=x>@example.com");
    hostile.with = Some(vec![String::from("<b>@example.com")]);
    let html = page(&hostile, "Text.\n", None);
    assert!(!html.contains("<script>alert"), "{html}");
    assert!(!html.contains("<img"), "{html}");
    assert!(!html.contains("<b>@"), "{html}");
}

#[test]
fn a_page_with_code_or_a_diagram_loads_the_entry_script_and_a_page_with_neither_does_not() {
    let entry = Some("reading-AbCd1234.js");
    let tag = "<script type=\"module\" src=\"/flare/assets/reading-AbCd1234.js\"></script>";
    assert!(page(&reading(), "```rust\nfn x() {}\n```\n", entry).contains(tag));
    assert!(page(&reading(), "```mermaid\npie\n```\n", entry).contains(tag));
    assert!(
        !page(
            &reading(),
            "Text and a table.\n\n| a |\n|---|\n| 1 |\n",
            entry
        )
        .contains("<script")
    );
    assert!(!page(&reading(), "```rust\nfn x() {}\n```\n", None).contains("<script"));
}

#[test]
fn initials_come_from_the_first_two_words_of_the_local_part() {
    assert_eq!(initials("priya.nair@example.com"), "PN");
    assert_eq!(initials("ana_okafor@example.com"), "AO");
    assert_eq!(initials("bob@example.com"), "BO");
    assert_eq!(initials("x@example.com"), "X");
    assert_eq!(initials("@example.com"), "?");
}

#[test]
fn a_date_shows_its_day_month_and_year() {
    assert_eq!(
        date(1_795_219_200),
        (
            String::from("21 Nov 2026"),
            String::from("2026-11-21T00:00:00Z")
        )
    );
    assert_eq!(date(0).0, "1 Jan 1970");
}

/// The page's foot, from its opening tag to its closing one.
fn foot(html: &str) -> &str {
    html.split_once("<footer class=\"share-foot\">")
        .and_then(|(_, rest)| rest.split_once("</footer>"))
        .map(|(foot, _)| foot)
        .unwrap()
}

#[test]
fn a_reader_who_has_never_shared_sees_the_note_and_the_get_flare_link_below_the_article() {
    let mut badged = reading();
    badged.badge = Some(String::from("https://example.com/flare"));
    let html = page(&badged, "Text.\n", None);
    let foot = foot(&html);
    assert!(
        foot.contains("<span class=\"note\">Shared with Flare</span>"),
        "{foot}"
    );
    assert!(foot.contains("<a class=\"badge\" href=\"https://example.com/flare\" target=\"_blank\" rel=\"noopener noreferrer\" aria-label=\"Get Flare, opens in a new tab\">"), "{foot}");
    assert!(foot.contains("<svg"), "{foot}");
    assert!(
        foot.contains(">Get Flare<span class=\"arr\" aria-hidden=\"true\">\u{2197}</span></a>"),
        "{foot}"
    );
    let article = html.find("</article>").unwrap();
    let footer = html.find("<footer class=\"share-foot\">").unwrap();
    assert!(article < footer);
}

#[test]
fn a_page_with_no_get_flare_address_shows_the_note_and_no_link() {
    let html = page(&reading(), "Text.\n", None);
    let foot = foot(&html);
    assert!(foot.contains("Shared with Flare"), "{foot}");
    assert!(!foot.contains("<a "), "{foot}");
    assert!(!html.contains("Get Flare"), "{html}");
}

#[test]
fn the_get_flare_address_is_escaped_in_the_link() {
    let mut hostile = reading();
    hostile.badge = Some(String::from(
        "https://example.com/flare?a=\"><script>x</script>&b=1",
    ));
    let html = page(&hostile, "Text.\n", None);
    assert!(!html.contains("<script>x"), "{html}");
    assert!(
        foot(&html).contains(
            "href=\"https://example.com/flare?a=&quot;&gt;&lt;script&gt;x&lt;/script&gt;&amp;b=1\""
        ),
        "{html}"
    );
}
