//! The content of an uploaded HTML share: one file in the file store.

use crate::error::{ParseError, Subject};
use crate::file::FileId;

/// The longest HTML file, in bytes: 4 MiB, which equals
/// [`MAX_UPLOAD_BYTES`](crate::MAX_UPLOAD_BYTES). The file is stored in the
/// file store, because a table item holds at most 400 KB, and a 4 MiB page
/// fits under the 6 MB reply that Lambda behind API Gateway sends.
pub const MAX_HTML: usize = 4 * 1024 * 1024;

/// The HTML file that one version of an `html` share holds: the id of the
/// stored file, which holds the bytes as uploaded, and its size in bytes
/// (1 to [`MAX_HTML`]). Nothing in this type reads or rewrites the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HtmlFile {
    file: FileId,
    size: u32,
}

impl HtmlFile {
    /// The file `file` of `size` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Empty`] for 0 bytes and [`ParseError::TooLong`]
    /// for more than [`MAX_HTML`].
    pub fn new(file: FileId, size: usize) -> Result<Self, ParseError> {
        let subject = Subject::HtmlFile;
        if size == 0 {
            return Err(ParseError::Empty { subject });
        }
        let size = u32::try_from(size)
            .ok()
            .filter(|size| usize::try_from(*size).is_ok_and(|size| size <= MAX_HTML))
            .ok_or(ParseError::TooLong {
                subject,
                max: MAX_HTML,
            })?;
        Ok(Self { file, size })
    }

    /// The stored file.
    #[must_use]
    pub fn file(&self) -> &FileId {
        &self.file
    }

    /// The file's size in bytes.
    #[must_use]
    pub fn size(&self) -> usize {
        usize::try_from(self.size).unwrap_or(usize::MAX)
    }
}
