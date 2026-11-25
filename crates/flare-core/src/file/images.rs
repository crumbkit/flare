//! The images a version of a share shows: each the path its Markdown wrote
//! and the stored file the server serves for it.

use super::{FileId, MediaType};
use crate::error::{ParseError, Subject};
use std::fmt;

/// The most images one version shows.
pub const MAX_IMAGES: usize = 10;

/// The longest image path, in bytes.
pub const MAX_IMAGE_PATH: usize = 255;

/// The path of an image as a document's Markdown writes it, such as
/// `img/chart.png`: 1 to 255 bytes of `/`-separated segments, with no
/// leading `/`, no empty, `.` or `..` segment, and no backslash, colon, NUL
/// or other control character. A path may name a person, so its `Debug`
/// output gives only its length.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImagePath(String);

impl ImagePath {
    /// The path as the Markdown wrote it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ImagePath {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::ImagePath;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        if text.len() > MAX_IMAGE_PATH {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_IMAGE_PATH,
            });
        }
        let bad_character = text
            .chars()
            .any(|character| character.is_control() || character == '\\' || character == ':');
        let bad_segment = text
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..");
        if bad_character || bad_segment {
            return Err(ParseError::Malformed {
                subject,
                rule: "a relative path with no empty, . or .. segment, backslash, colon or control character",
            });
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Debug for ImagePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ImagePath(<{} bytes>)", self.0.len())
    }
}

/// One row of a version's image table: the path its Markdown wrote, the
/// stored file, its media type and its size in bytes once stripped.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VersionImage {
    path: ImagePath,
    file: FileId,
    media_type: MediaType,
    size: u32,
}

impl VersionImage {
    /// A row naming `file` for `path`.
    #[must_use]
    pub fn new(path: ImagePath, file: FileId, media_type: MediaType, size: u32) -> Self {
        Self {
            path,
            file,
            media_type,
            size,
        }
    }

    /// The path the Markdown wrote.
    #[must_use]
    pub fn path(&self) -> &ImagePath {
        &self.path
    }

    /// The stored file.
    #[must_use]
    pub fn file(&self) -> &FileId {
        &self.file
    }

    /// The stored file's media type.
    #[must_use]
    pub fn media_type(&self) -> &MediaType {
        &self.media_type
    }

    /// The stored file's size in bytes.
    #[must_use]
    pub fn size(&self) -> u32 {
        self.size
    }
}

/// A version's image table: at most [`MAX_IMAGES`] rows, no path and no
/// file named twice. A version stored before images existed has none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct VersionImages(Vec<VersionImage>);

impl VersionImages {
    /// The empty table.
    #[must_use]
    pub fn none() -> Self {
        Self(Vec::new())
    }

    /// The rows, in the order the request named them.
    #[must_use]
    pub fn rows(&self) -> &[VersionImage] {
        &self.0
    }

    /// Whether the table has no row.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The row that names `file`, if any.
    #[must_use]
    pub fn by_file(&self, file: &FileId) -> Option<&VersionImage> {
        self.0.iter().find(|row| row.file() == file)
    }

    /// Whether a row names `file`.
    #[must_use]
    pub fn names(&self, file: &FileId) -> bool {
        self.by_file(file).is_some()
    }
}

impl TryFrom<Vec<VersionImage>> for VersionImages {
    type Error = ParseError;

    fn try_from(rows: Vec<VersionImage>) -> Result<Self, ParseError> {
        let subject = Subject::VersionImages;
        if rows.len() > MAX_IMAGES {
            return Err(ParseError::TooMany {
                subject,
                max: MAX_IMAGES,
            });
        }
        for (index, row) in rows.iter().enumerate() {
            let earlier = rows.get(..index).unwrap_or_default();
            if earlier
                .iter()
                .any(|other| other.path == row.path || other.file == row.file)
            {
                return Err(ParseError::Duplicate { subject });
            }
        }
        Ok(Self(rows))
    }
}

#[cfg(test)]
mod tests;
