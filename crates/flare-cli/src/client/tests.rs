//! Unit tests of the form body a share with images is sent as.

use super::{boundary, form};
use crate::sharing::{LocalPart, Part};

fn image(path: &str, bytes: &[u8]) -> LocalPart {
    LocalPart {
        part: Part::Image,
        path: path.to_owned(),
        bytes: bytes.to_vec(),
    }
}

#[test]
fn a_form_holds_the_share_part_then_one_image_part_per_image_named_by_its_path() {
    let images = [image("img/a.png", b"AAA"), image("b.png", b"")];
    let body = form("flare-form-0", br#"{"title":"Notes"}"#, &images);
    let expected = concat!(
        "--flare-form-0\r\nContent-Disposition: form-data; name=\"share\"\r\nContent-Type: application/json\r\n\r\n",
        r#"{"title":"Notes"}"#,
        "\r\n--flare-form-0\r\nContent-Disposition: form-data; name=\"image\"; filename=\"img/a.png\"\r\nContent-Type: application/octet-stream\r\n\r\nAAA",
        "\r\n--flare-form-0\r\nContent-Disposition: form-data; name=\"image\"; filename=\"b.png\"\r\nContent-Type: application/octet-stream\r\n\r\n",
        "\r\n--flare-form-0--\r\n",
    );
    assert_eq!(String::from_utf8(body).unwrap(), expected);
}

#[test]
fn a_filename_escapes_its_quotes_and_backslashes() {
    let body = form("b", b"{}", &[image("a\"b\\c.png", b"")]);
    let text = String::from_utf8(body).unwrap();
    assert!(text.contains(r#"filename="a\"b\\c.png""#), "{text}");
}

#[test]
fn the_boundary_is_one_that_no_part_holds() {
    let first = boundary(b"{}", &[image("a.png", b"plain")]);
    assert_eq!(first, "flare-form-0000000000000000");
    let holding = format!("x{first}y");
    let second = boundary(b"{}", &[image("a.png", holding.as_bytes())]);
    assert_eq!(second, "flare-form-0000000000000001");
    let in_share = boundary(holding.as_bytes(), &[]);
    assert_eq!(in_share, "flare-form-0000000000000001");
}

#[test]
fn an_html_file_is_the_part_named_html_with_the_content_type_text_html() {
    let file = LocalPart {
        part: Part::Html,
        path: "plan.html".to_owned(),
        bytes: b"<p>x</p>".to_vec(),
    };
    let body = String::from_utf8(form("b", b"{}", &[file])).unwrap();
    assert!(
        body.contains(
            "name=\"html\"; filename=\"plan.html\"\r\nContent-Type: text/html\r\n\r\n<p>x</p>"
        ),
        "{body}"
    );
}
