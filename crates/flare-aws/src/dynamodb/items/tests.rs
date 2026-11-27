//! Unit tests of the version item of an HTML share.

use super::{Item, attr, text, version_from, version_item};
use flare_core::{
    DocumentText, FileId, HtmlFile, PrincipalId, ShareContent, ShareId, ShareVersion,
    SharedVersion, Timestamp,
};

fn version(content: ShareContent) -> SharedVersion {
    SharedVersion::new(
        ShareId::from_random([1; 16]),
        ShareVersion::first(),
        content,
        PrincipalId::try_from("owner@example.com").unwrap(),
        Timestamp::from_secs(20),
    )
}

fn html(size: usize) -> ShareContent {
    ShareContent::Html(HtmlFile::new(FileId::from_random([5; 16]), size).unwrap())
}

#[test]
fn an_html_version_item_holds_the_file_id_and_size_and_no_text() {
    let item = version_item(&version(html(1_234))).unwrap();
    assert_eq!(item.get(attr::KIND), Some(&text("html")));
    assert_eq!(item.get(attr::HTML_FILE), Some(&text(&"05".repeat(16))));
    assert!(item.contains_key(attr::HTML_SIZE));
    assert!(!item.contains_key(attr::TEXT));
}

#[test]
fn an_html_version_reads_back_from_its_item() {
    let written = version(html(4 * 1024 * 1024));
    let item = version_item(&written).unwrap();
    let read = version_from(&item, written.share(), written.number()).unwrap();
    assert_eq!(read, written);
}

#[test]
fn a_markdown_version_still_reads_back_from_its_item() {
    let written = version(ShareContent::Document(
        DocumentText::try_from("# Notes\n").unwrap(),
    ));
    let item = version_item(&written).unwrap();
    assert_eq!(item.get(attr::KIND), Some(&text("document")));
    let read = version_from(&item, written.share(), written.number()).unwrap();
    assert_eq!(read, written);
}

#[test]
fn an_html_item_without_a_file_id_is_malformed() {
    let written = version(html(10));
    let mut item: Item = version_item(&written).unwrap();
    item.remove(attr::HTML_FILE);
    assert!(version_from(&item, written.share(), written.number()).is_err());
}
