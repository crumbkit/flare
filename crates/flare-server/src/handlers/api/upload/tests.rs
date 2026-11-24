//! Unit tests of reading an upload from its form's parts.

use super::super::ApiRoute;
use super::super::multipart::Part;
use super::upload_of;
use crate::sharing::Upload;
use flare_contract::ApiCode;
use hyper::body::Bytes;

fn part(name: &str, filename: Option<&str>, data: &'static [u8]) -> Part {
    Part {
        name: name.to_owned(),
        filename: filename.map(str::to_owned),
        data: Bytes::from_static(data),
    }
}

const SHARE: &[u8] = br#"{"title":"Notes","format":"markdown","content":"![a](a.png)\n"}"#;
const VERSION: &[u8] = br#"{"format":"markdown","content":"![a](a.png)\n"}"#;

#[test]
fn a_create_form_reads_as_its_share_and_its_images_in_order() {
    let upload = upload_of(
        ApiRoute::CreateShare,
        "",
        vec![
            part("image", Some("a.png"), b"one"),
            part("share", None, SHARE),
            part("image", Some("img/b.png"), b"two"),
        ],
    )
    .unwrap();
    let Upload::Create(request, images) = upload else {
        panic!("{upload:?}")
    };
    assert_eq!(request.title, "Notes");
    let paths: Vec<&str> = images.iter().map(|image| image.path.as_str()).collect();
    assert_eq!(paths, ["a.png", "img/b.png"]);
    assert_eq!(images[1].bytes, Bytes::from_static(b"two"));
}

#[test]
fn a_publish_form_names_its_share() {
    let upload = upload_of(
        ApiRoute::PublishVersion,
        "0123456789abcdef0123456789abcdef",
        vec![part("share", None, VERSION)],
    )
    .unwrap();
    assert!(
        matches!(upload, Upload::Publish(id, _, images) if id == "0123456789abcdef0123456789abcdef" && images.is_empty())
    );
}

#[test]
fn a_form_with_no_share_two_shares_a_nameless_image_or_another_part_is_a_bad_body() {
    for parts in [
        vec![part("image", Some("a.png"), b"one")],
        vec![part("share", None, SHARE), part("share", None, SHARE)],
        vec![part("share", None, SHARE), part("image", None, b"one")],
        vec![part("share", None, SHARE), part("note", None, b"hi")],
        vec![part("share", Some("share.json"), SHARE)],
        vec![part("share", None, b"{")],
    ] {
        let refusal = upload_of(ApiRoute::CreateShare, "", parts).unwrap_err();
        assert_eq!(refusal.code, ApiCode::BadBody);
    }
}
