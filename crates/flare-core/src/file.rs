//! Files that a share's pages show, such as images, each stored under one
//! share.

mod images;
mod kind;
mod sweep;

pub use images::{ImagePath, MAX_IMAGE_PATH, MAX_IMAGES, VersionImage, VersionImages};
pub use kind::{ImageKind, SNIFF_BYTES};
pub use sweep::{MAX_SWEPT_FILES, SWEEP_AGE, SweepReport, sweep_files};

use crate::error::{ParseError, Subject};
use crate::share::{ShareId, check_hex_id, hex_id};
use std::fmt;

/// The largest file, in bytes: 10 MiB.
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

/// The most bytes of one share or version sent with its images as a
/// `multipart/form-data` body: 4 MiB, because Lambda behind API Gateway
/// receives at most about 4.5 MiB. The server's `Limits::upload_bytes`
/// defaults to it and answers a larger body with 413
/// `request::body_too_large`, and `flare share` refuses a larger body
/// before it sends one.
pub const MAX_UPLOAD_BYTES: usize = 4 * 1024 * 1024;

/// The longest media type, in bytes.
const MAX_MEDIA_TYPE: usize = 100;

/// The id of one stored file: 128 random bits as 32 lower-case hexadecimal
/// characters, which the caller draws.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(String);

impl FileId {
    /// The id that 16 random bytes give.
    #[must_use]
    pub fn from_random(bytes: [u8; 16]) -> Self {
        Self(hex_id(bytes))
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for FileId {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        check_hex_id(Subject::FileId, text)?;
        Ok(Self(text.to_owned()))
    }
}

/// Whether `name` is 1 or more of the characters that a media type's type
/// and subtype names may hold, in lower case.
fn is_media_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"!#$&^_.+-".contains(&byte)
        })
}

/// A media type such as `image/png`: a type and a subtype joined by one
/// `/`, at most 100 bytes, held in lower case.
///
/// Shortcut: it takes no parameters, such as `charset`, and admits any
/// type and subtype. Which types a share may serve is decided where files
/// are written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaType(String);

impl MediaType {
    /// The media type as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for MediaType {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::MediaType;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        if text.len() > MAX_MEDIA_TYPE {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_MEDIA_TYPE,
            });
        }
        let lower = text.to_ascii_lowercase();
        let mut names = lower.split('/');
        match (names.next(), names.next(), names.next()) {
            (Some(kind), Some(sub), None) if is_media_name(kind) && is_media_name(sub) => {
                Ok(Self(lower))
            }
            _ => Err(ParseError::Malformed {
                subject,
                rule: "a type and a subtype joined by one slash",
            }),
        }
    }
}

/// The bytes of one file: at most [`MAX_FILE_BYTES`]. Its `Debug` output
/// gives only its length.
#[derive(Clone, PartialEq, Eq)]
pub struct FileBytes(Vec<u8>);

impl FileBytes {
    /// The bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<Vec<u8>> for FileBytes {
    type Error = ParseError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, ParseError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(ParseError::TooLong {
                subject: Subject::FileBytes,
                max: MAX_FILE_BYTES,
            });
        }
        Ok(Self(bytes))
    }
}

impl fmt::Debug for FileBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "FileBytes(<{} bytes>)", self.0.len())
    }
}

/// A file to store under a share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFile {
    share: ShareId,
    id: FileId,
    media_type: MediaType,
    bytes: FileBytes,
}

impl NewFile {
    /// A file with the id `id` under the share `share`.
    #[must_use]
    pub fn new(share: ShareId, id: FileId, media_type: MediaType, bytes: FileBytes) -> Self {
        Self {
            share,
            id,
            media_type,
            bytes,
        }
    }

    /// The share it is stored under.
    #[must_use]
    pub fn share(&self) -> &ShareId {
        &self.share
    }

    /// Its id.
    #[must_use]
    pub fn id(&self) -> &FileId {
        &self.id
    }

    /// Its media type.
    #[must_use]
    pub fn media_type(&self) -> &MediaType {
        &self.media_type
    }

    /// Its bytes.
    #[must_use]
    pub fn bytes(&self) -> &FileBytes {
        &self.bytes
    }
}

/// A file as a store read it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile(NewFile);

impl StoredFile {
    /// The share it is stored under.
    #[must_use]
    pub fn share(&self) -> &ShareId {
        self.0.share()
    }

    /// Its id.
    #[must_use]
    pub fn id(&self) -> &FileId {
        self.0.id()
    }

    /// Its media type.
    #[must_use]
    pub fn media_type(&self) -> &MediaType {
        self.0.media_type()
    }

    /// Its bytes.
    #[must_use]
    pub fn bytes(&self) -> &FileBytes {
        self.0.bytes()
    }
}

impl From<NewFile> for StoredFile {
    /// The file that a store holds after it stores `file`.
    fn from(file: NewFile) -> Self {
        Self(file)
    }
}

#[cfg(test)]
mod tests;
