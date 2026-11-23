//! Writes an edit's fields into a record's JSON text and leaves every other
//! byte of the text as it is: key order, spacing, number spellings and
//! integers too large for a 64-bit number all stay.

/// One top-level member of a JSON object's text, as byte offsets.
#[derive(Debug)]
struct Member {
    key: String,
    /// Where the member's key starts.
    start: usize,
    /// Where its value starts.
    value: usize,
    /// Where its value ends.
    end: usize,
}

/// The members of the JSON object `text` and the offset just after its
/// opening brace, or `None` when `text` is not one JSON object.
fn members(text: &str) -> Option<(usize, Vec<Member>)> {
    let bytes = text.as_bytes();
    let mut at = skip_space(bytes, 0);
    if bytes.get(at) != Some(&b'{') {
        return None;
    }
    let open = at + 1;
    at = skip_space(bytes, open);
    let mut found = Vec::new();
    if bytes.get(at) == Some(&b'}') {
        return Some((open, found));
    }
    loop {
        let start = at;
        let key_end = string_end(bytes, start)?;
        let key: String = serde_json::from_str(text.get(start..key_end)?).ok()?;
        at = skip_space(bytes, key_end);
        if bytes.get(at) != Some(&b':') {
            return None;
        }
        let value = skip_space(bytes, at + 1);
        let (end, next) = value_end(bytes, value)?;
        found.push(Member {
            key,
            start,
            value,
            end,
        });
        match bytes.get(next) {
            Some(b',') => at = skip_space(bytes, next + 1),
            Some(b'}') => return Some((open, found)),
            _ => return None,
        }
    }
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(|byte| b" \t\r\n".contains(byte)) {
        at += 1;
    }
    at
}

/// The offset just after the JSON string that starts at `start`.
fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let mut at = start + 1;
    loop {
        match bytes.get(at)? {
            b'\\' => at += 2,
            b'"' => return Some(at + 1),
            _ => at += 1,
        }
    }
}

/// Where the value that starts at `start` ends, with trailing space left
/// out, and where the `,` or `}` after it is.
fn value_end(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut depth = 0_usize;
    let mut at = start;
    let mut end = start;
    loop {
        match bytes.get(at)? {
            b'"' => {
                at = string_end(bytes, at)?;
                end = at;
                continue;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' if depth > 0 => depth -= 1,
            b',' | b'}' if depth == 0 => return Some((end, at)),
            byte if b" \t\r\n".contains(byte) => {
                at += 1;
                continue;
            }
            _ => {}
        }
        at += 1;
        end = at;
    }
}

/// `text` with each field in `changes` set to its JSON value, or removed for
/// `None`. A field the text lacks is added after the last member. Every
/// other byte stays. Returns `None` when `text` is not one JSON object.
pub(super) fn splice(text: &str, changes: &[(&str, Option<String>)]) -> Option<String> {
    let (open, found) = members(text)?;
    let change = |key: &str| changes.iter().find(|(name, _)| *name == key);
    let added: Vec<String> = changes
        .iter()
        .filter(|(name, _)| !found.iter().any(|member| member.key == *name))
        .filter_map(|(name, value)| {
            let key = serde_json::Value::String((*name).to_owned()).to_string();
            value.as_ref().map(|value| format!("{key}: {value}"))
        })
        .collect();
    let (Some(first), Some(last)) = (found.first(), found.last()) else {
        let joined = added.join(", ");
        return Some(format!(
            "{}{joined}{}",
            text.get(..open)?,
            text.get(open..)?
        ));
    };
    let separator = |index: usize| -> Option<&str> {
        let next = found.get(index + 1)?;
        text.get(found.get(index)?.end..next.start)
    };
    let default = match separator(0) {
        Some(separator) => separator.to_owned(),
        None => {
            let lead = text.get(open..first.start)?;
            match lead.rfind('\n') {
                Some(newline) => format!(",{}", lead.get(newline..)?),
                None => ", ".to_owned(),
            }
        }
    };
    let mut out = String::with_capacity(text.len());
    out.push_str(text.get(..first.start)?);
    let mut previous: Option<usize> = None;
    for (index, member) in found.iter().enumerate() {
        let written = match change(&member.key) {
            Some((_, None)) => continue,
            Some((_, Some(value))) => format!("{}{value}", text.get(member.start..member.value)?),
            None => text.get(member.start..member.end)?.to_owned(),
        };
        if let Some(before) = previous {
            out.push_str(separator(before).unwrap_or(&default));
        }
        out.push_str(&written);
        previous = Some(index);
    }
    for member in &added {
        if previous.is_some() {
            out.push_str(&default);
        }
        out.push_str(member);
        previous = Some(found.len());
    }
    out.push_str(text.get(last.end..)?);
    Some(out)
}

#[cfg(test)]
mod tests;
