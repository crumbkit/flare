//! `hosted.teams`, which says where the share API's teams come from.
//!
//! Left out, or `"stored"`, flare keeps the teams in its own table, and
//! people create and change them with `flare teams`. `{"file": "<path>"}`
//! reads them from a teams document at an absolute path, and
//! `{"object": "teams/<name>"}` reads it from that object in the stores
//! bucket. The deployment's own sync job writes the document, such as from
//! its directory, and flare only reads it: every team change then answers
//! `flare::teams::managed_elsewhere`.

use flare_aws::TeamsObjectKey;
use serde_json::Value;
use std::path::PathBuf;

/// The value of `hosted.teams` that keeps flare's own teams.
const STORED: &str = "stored";

/// Where the share API's teams come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TeamsSource {
    /// flare's own table.
    Stored,
    /// The teams document at this absolute path.
    File(PathBuf),
    /// The teams document at this key of the stores bucket.
    Object(TeamsObjectKey),
}

/// The source that `value`, the value of `hosted.teams`, names.
///
/// # Errors
///
/// Returns the refusal, naming `hosted.teams`.
pub(crate) fn teams(value: Option<&Value>) -> Result<TeamsSource, String> {
    let bad = || {
        "hosted.teams must be \"stored\", {\"file\": \"<absolute path>\"} or {\"object\": \"teams/<name>\"}, where the object is in the stores bucket".to_owned()
    };
    let Some(value) = value else {
        return Ok(TeamsSource::Stored);
    };
    let object = match value {
        Value::String(word) if word == STORED => return Ok(TeamsSource::Stored),
        Value::Object(object) if object.len() == 1 => object,
        _ => return Err(bad()),
    };
    match (object.get("file"), object.get("object")) {
        (Some(Value::String(path)), None) => {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                Ok(TeamsSource::File(path))
            } else {
                Err("hosted.teams.file must be an absolute path".to_owned())
            }
        }
        (None, Some(Value::String(key))) => TeamsObjectKey::try_from(key.as_str())
            .map(TeamsSource::Object)
            .map_err(|error| format!("hosted.teams.object is not valid: {error}")),
        _ => Err(bad()),
    }
}
