//! A reader of the `multipart/form-data` body (RFC 7578) that `flare
//! share` sends with a document's images.
//!
//! Shortcut: this reader is written by hand, because a multipart crate is
//! a new dependency and needs its own eval. It reads the parts the flare
//! command writes and refuses anything else: CRLF line ends, a boundary
//! of 1 to 70 characters, a body that starts with the first delimiter,
//! and parts whose headers are a `Content-Disposition: form-data` with a
//! quoted `name` and an optional quoted `filename`, and an optional
//! `Content-Type`. A quoted value may escape a quote or a backslash with a
//! backslash. A part's bytes are a slice of the body, so reading copies
//! nothing. The body is already bounded by `Limits::upload_bytes`.

use hyper::body::Bytes;

/// The longest boundary RFC 2046 allows.
const MAX_BOUNDARY: usize = 70;

/// The most bytes of one part's header block.
const MAX_HEADERS: usize = 4096;

/// One part of a form: its name, its filename when it has one, and its
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Part {
    pub(crate) name: String,
    pub(crate) filename: Option<String>,
    pub(crate) data: Bytes,
}

/// The boundary of a `multipart/form-data` Content-Type, or `None` when
/// the type is another or names no usable boundary.
pub(crate) fn boundary(content_type: &str) -> Option<String> {
    let mut pieces = content_type.split(';');
    let media = pieces.next()?.trim();
    if !media.eq_ignore_ascii_case("multipart/form-data") {
        return None;
    }
    let value = pieces.find_map(|piece| {
        let (key, value) = piece.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("boundary")
            .then(|| value.trim())
    })?;
    let value = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value);
    let usable = (1..=MAX_BOUNDARY).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
        && !value.ends_with(' ');
    usable.then(|| value.to_owned())
}

/// Where `needle` first appears in `haystack` at or after `from`.
fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .and_then(|at| at.checked_add(from))
}

/// The parts of `body`, whose boundary is `boundary`, or `None` when the
/// body is not a form this reader reads.
pub(crate) fn parse(body: &Bytes, boundary: &str) -> Option<Vec<Part>> {
    let delimiter = format!("--{boundary}");
    let next_delimiter = format!("\r\n--{boundary}");
    if !body.starts_with(delimiter.as_bytes()) {
        return None;
    }
    let mut at = delimiter.len();
    let mut parts = Vec::new();
    loop {
        let rest = body.get(at..)?;
        if rest.starts_with(b"--") {
            return Some(parts);
        }
        if !rest.starts_with(b"\r\n") {
            return None;
        }
        let headers_start = at.checked_add(2)?;
        let headers_end = find(body, b"\r\n\r\n", headers_start)?;
        if headers_end.checked_sub(headers_start)? > MAX_HEADERS {
            return None;
        }
        let headers = std::str::from_utf8(body.get(headers_start..headers_end)?).ok()?;
        let (name, filename) = disposition(headers)?;
        let data_start = headers_end.checked_add(4)?;
        let data_end = find(body, next_delimiter.as_bytes(), data_start)?;
        parts.push(Part {
            name,
            filename,
            data: body.slice(data_start..data_end),
        });
        at = data_end.checked_add(next_delimiter.len())?;
    }
}

/// The name and filename that a part's header block gives.
fn disposition(headers: &str) -> Option<(String, Option<String>)> {
    let mut found = None;
    for line in headers.split("\r\n") {
        let (key, value) = line.split_once(':')?;
        if key.eq_ignore_ascii_case("content-disposition") {
            if found.is_some() {
                return None;
            }
            found = Some(form_data(value.trim())?);
        } else if !key.eq_ignore_ascii_case("content-type") {
            return None;
        }
    }
    found
}

/// The `name` and `filename` of a `form-data` disposition value.
fn form_data(value: &str) -> Option<(String, Option<String>)> {
    let rest = value.strip_prefix("form-data")?;
    let mut name = None;
    let mut filename = None;
    let mut rest = rest;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        rest = rest.strip_prefix(';')?.trim_start();
        let (key, after) = rest.split_once('=')?;
        let (text, after) = quoted(after)?;
        match key.trim() {
            "name" if name.is_none() => name = Some(text),
            "filename" if filename.is_none() => filename = Some(text),
            _ => return None,
        }
        rest = after;
    }
    Some((name?, filename))
}

/// A quoted string at the start of `text`, unescaped, and the text after
/// it.
fn quoted(text: &str) -> Option<(String, &str)> {
    let body = text.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = body.char_indices();
    while let Some((index, character)) = chars.next() {
        match character {
            '"' => return Some((value, body.get(index.checked_add(1)?..)?)),
            '\\' => value.push(chars.next()?.1),
            _ => value.push(character),
        }
    }
    None
}

#[cfg(test)]
mod tests;
