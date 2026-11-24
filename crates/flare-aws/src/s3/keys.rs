//! The object key of a file, built only from parsed ids.

use flare_core::{FileId, ShareId};

/// `files/<share id>/<file id>`. Both ids are 32 lower-case hexadecimal
/// characters, so every key has the same shape and names one file of one
/// share.
pub(crate) fn object_key(share: &ShareId, id: &FileId) -> String {
    format!("files/{}/{}", share.as_str(), id.as_str())
}

/// The share and file that a key of the shape [`object_key`] writes names,
/// or `None` for any other key.
pub(crate) fn file_of_key(key: &str) -> Option<(ShareId, FileId)> {
    let rest = key.strip_prefix(FILES_PREFIX)?;
    let (share, id) = rest.split_once('/')?;
    Some((ShareId::try_from(share).ok()?, FileId::try_from(id).ok()?))
}

/// The prefix of every file's key.
pub(crate) const FILES_PREFIX: &str = "files/";
