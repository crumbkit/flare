//! The object key of a file, built only from parsed ids.

use flare_core::{FileId, ShareId};

/// `files/<share id>/<file id>`. Both ids are 32 lower-case hexadecimal
/// characters, so every key has the same shape and names one file of one
/// share.
pub(crate) fn object_key(share: &ShareId, id: &FileId) -> String {
    format!("files/{}/{}", share.as_str(), id.as_str())
}
