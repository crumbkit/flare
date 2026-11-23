//! The embedded files. A build with no bundle embeds none; a build with
//! `FLARE_REQUIRE_UI=1`, as in CI's reading job,
//! checks every file Vite wrote.

use super::*;

/// The largest file the server may embed: 5 MB, below the 6 MB response
/// limit of AWS Lambda, which serves the hosted site. The remedy for a
/// larger file is to split the chunk in Vite.
const MAX_FILE_BYTES: usize = 5_000_000;

#[test]
fn no_embedded_file_is_over_5_mb() {
    for (name, bytes) in files() {
        assert!(
            bytes.len() <= MAX_FILE_BYTES,
            "{name} is {} bytes; split the chunk in ui/flare/vite.config.ts",
            bytes.len()
        );
    }
}

#[test]
fn every_embedded_file_name_holds_a_content_hash() {
    // Vite ends each name with a hyphen and 8 characters of base64url.
    for (name, _) in files() {
        let (stem, _) = name.rsplit_once('.').unwrap();
        let bytes = stem.as_bytes();
        let split = bytes.len().checked_sub(9).unwrap();
        assert_eq!(bytes[split], b'-', "{name}");
        assert!(
            bytes[split + 1..]
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-'),
            "{name}"
        );
    }
}

#[test]
fn the_entry_is_one_of_the_embedded_files() {
    match entry() {
        Some(entry) => assert!(files().iter().any(|(name, _)| *name == entry)),
        None => assert!(files().is_empty()),
    }
}

#[test]
fn an_embedded_file_is_served_with_an_immutable_cache_and_an_unknown_name_is_not_found() {
    if let Some((name, bytes)) = files().first() {
        let reply = serve(name).unwrap();
        assert_eq!(reply.status(), StatusCode::OK);
        let response = reply.into_response(crate::site::SiteKind::Hosted);
        assert_eq!(
            response.headers()["cache-control"],
            "public, max-age=31536000, immutable"
        );
        assert_eq!(response.headers()["content-type"], media_type(name));
        assert!(!bytes.is_empty());
    }
    assert!(serve("reading-00000000.js").is_err());
    assert!(serve("../Cargo.toml").is_err());
}

#[test]
fn a_script_is_served_as_javascript() {
    assert_eq!(
        media_type("reading-BIKRgxqI.js"),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(media_type("x-12345678.css"), "text/css; charset=utf-8");
    assert_eq!(media_type("x-12345678"), "application/octet-stream");
}

#[test]
fn each_vendored_font_is_a_woff2_file_named_for_its_hash() {
    for (name, bytes) in FONTS {
        assert_eq!(&bytes[..4], b"wOF2", "{name}");
        let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, bytes);
        let hex: String = digest.as_ref()[..4]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert!(name.ends_with(&format!("-{hex}.woff2")), "{name} vs {hex}");
    }
}

#[test]
fn a_font_is_served_as_woff2_with_an_immutable_cache() {
    for (name, bytes) in FONTS {
        let reply = serve(name).unwrap();
        assert_eq!(reply.status(), StatusCode::OK);
        let response = reply.into_response(crate::site::SiteKind::Hosted);
        assert_eq!(response.headers()["content-type"], "font/woff2");
        assert_eq!(
            response.headers()["cache-control"],
            "public, max-age=31536000, immutable"
        );
        assert!(!bytes.is_empty());
    }
}

#[test]
fn the_licence_of_each_font_family_sits_beside_its_file() {
    let fonts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts");
    for licence in ["IBM-Plex-Sans-OFL.txt", "JetBrains-Mono-OFL.txt"] {
        let text = std::fs::read_to_string(fonts.join(licence)).unwrap();
        assert!(
            text.contains("SIL OPEN FONT LICENSE Version 1.1"),
            "{licence}"
        );
    }
}
