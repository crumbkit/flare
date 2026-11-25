//! Unit tests of the multipart reader.

use super::{Part, boundary, parse};
use hyper::body::Bytes;

const BOUNDARY: &str = "flare-7d3c";

/// A form of `parts`, each a name, an optional filename and bytes.
fn form(parts: &[(&str, Option<&str>, &[u8])]) -> Bytes {
    let mut body = Vec::new();
    for (name, filename, data) in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        let disposition = match filename {
            Some(file) => format!("form-data; name=\"{name}\"; filename=\"{file}\""),
            None => format!("form-data; name=\"{name}\""),
        };
        body.extend_from_slice(format!("Content-Disposition: {disposition}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    Bytes::from(body)
}

#[test]
fn a_form_data_content_type_gives_its_boundary_and_any_other_type_gives_none() {
    for (header, expected) in [
        (
            "multipart/form-data; boundary=flare-7d3c",
            Some("flare-7d3c"),
        ),
        ("Multipart/Form-Data;boundary=\"a b\"", Some("a b")),
        ("application/json", None),
        ("multipart/form-data", None),
        ("multipart/form-data; boundary=", None),
        ("multipart/mixed; boundary=x", None),
    ] {
        assert_eq!(boundary(header).as_deref(), expected, "{header}");
    }
    let long = format!("multipart/form-data; boundary={}", "b".repeat(71));
    assert_eq!(boundary(&long), None);
}

#[test]
fn a_form_reads_as_its_parts_with_their_bytes_unchanged() {
    let binary: &[u8] = b"\x89PNG\r\n\x1a\n\r\n--flare-7d3 not the end\r\n";
    let body = form(&[
        ("share", None, br#"{"title":"Notes"}"#),
        ("image", Some("img/a b.png"), binary),
        ("image", Some("c.png"), b""),
    ]);
    let parts = parse(&body, BOUNDARY).unwrap();
    assert_eq!(
        parts,
        vec![
            Part {
                name: "share".to_owned(),
                filename: None,
                data: Bytes::from_static(br#"{"title":"Notes"}"#),
            },
            Part {
                name: "image".to_owned(),
                filename: Some("img/a b.png".to_owned()),
                data: Bytes::copy_from_slice(binary),
            },
            Part {
                name: "image".to_owned(),
                filename: Some("c.png".to_owned()),
                data: Bytes::new(),
            },
        ]
    );
}

#[test]
fn a_filename_may_escape_a_quote_and_a_backslash() {
    let body = Bytes::from(format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"a\\\"b\\\\c.png\"\r\n\r\nx\r\n--{BOUNDARY}--"
    ));
    let parts = parse(&body, BOUNDARY).unwrap();
    assert_eq!(parts[0].filename.as_deref(), Some("a\"b\\c.png"));
}

#[test]
fn a_body_that_is_not_a_well_formed_form_reads_as_none() {
    let good = form(&[("share", None, b"{}")]);
    assert!(parse(&good, BOUNDARY).is_some());
    let text = String::from_utf8(good.to_vec()).unwrap();
    for bad in [
        text.replacen("--flare-7d3c\r\n", "preamble\r\n--flare-7d3c\r\n", 1),
        text.replace("--flare-7d3c--\r\n", ""),
        text.replace("Content-Disposition", "Content-Location"),
        text.replace("form-data; name=\"share\"", "attachment; name=\"share\""),
        text.replace("name=\"share\"", "name=share"),
        text.replace("name=\"share\"", "name=\"share\"; size=\"3\""),
        text.replace("\r\n\r\n", "\r\n"),
        text.replace(
            "Content-Type: application/octet-stream\r\n",
            "X-Other: 1\r\n",
        ),
    ] {
        assert_eq!(parse(&Bytes::from(bad.clone()), BOUNDARY), None, "{bad:?}");
    }
}
