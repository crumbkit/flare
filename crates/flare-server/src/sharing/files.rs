//! The file store a [`Sharing`] keeps images in: [`NoFiles`] until
//! [`Sharing::with_files`] sets one, and the file route's read.

use super::{ReadRefusal, Sharing};
use flare_core::{
    CommentStore, Creators, CursorStore, FileCursor, FileId, FilePage, FileStore, FileStoreError,
    GrantStore, NewFile, Principal, ResponseStore, ShareError, ShareId, ShareStore, ShareVersion,
    StoredFile, Teams,
};

/// The file store of a site that stores no images. A put answers
/// `Unavailable`, so an upload with an image is refused before anything is
/// written, and a read finds no file.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct NoFiles;

impl FileStore for NoFiles {
    async fn put(&self, _file: NewFile) -> Result<(), FileStoreError> {
        Err(FileStoreError::Unavailable)
    }

    async fn get(&self, _share: &ShareId, _id: &FileId) -> Result<StoredFile, FileStoreError> {
        Err(FileStoreError::NotFound)
    }

    async fn delete(&self, _share: &ShareId, _id: &FileId) -> Result<(), FileStoreError> {
        Ok(())
    }

    async fn list(
        &self,
        _after: Option<&FileCursor>,
        _limit: usize,
    ) -> Result<FilePage, FileStoreError> {
        Ok(FilePage::new(Vec::new(), None))
    }
}

impl<S, G, R, C, U, T, X, M, K, F> Sharing<S, G, R, C, U, T, X, M, K, F> {
    /// The same share API keeping the images of shares in `files`.
    #[must_use]
    pub fn with_files<V: FileStore>(self, files: V) -> Sharing<S, G, R, C, U, T, X, M, K, V> {
        Sharing {
            flares: self.flares,
            files,
            tokens: self.tokens,
            recipients: self.recipients,
            admit: self.admit,
            get_flare: self.get_flare,
        }
    }
}

impl<S, G, R, C, U, T, X, M, K, F> Sharing<S, G, R, C, U, T, X, M, K, F>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
    M: Teams,
    K: Creators,
    F: FileStore,
{
    /// File `file` of version `version` of share `id`, for a caller who
    /// may open that version and only when the version's image table
    /// lists the file. Every other case, a malformed id or number
    /// included, is the same `NotFound`.
    pub(super) async fn image_file(
        &self,
        who: &Principal,
        id: &str,
        version: &str,
        file: &str,
    ) -> Result<StoredFile, ReadRefusal> {
        let id = ShareId::try_from(id).map_err(|_| ReadRefusal::NotFound)?;
        let number = version
            .parse::<u32>()
            .ok()
            .and_then(|number| ShareVersion::try_from(number).ok())
            .ok_or(ReadRefusal::NotFound)?;
        let file = FileId::try_from(file).map_err(|_| ReadRefusal::NotFound)?;
        self.flares
            .shares()
            .file(who, &self.files, &id, number, &file)
            .await
            .map_err(|error| match error {
                ShareError::Unavailable => ReadRefusal::Unavailable,
                _ => ReadRefusal::NotFound,
            })
    }

    /// The HTML file of version `version` of share `id`, for a viewer whom
    /// the admit rule admits. Every refusal is the same `NotFound`.
    pub(super) async fn html_file(
        &self,
        who: &Principal,
        id: &str,
        version: u32,
    ) -> Result<StoredFile, ReadRefusal> {
        let admitted = who
            .person()
            .is_some_and(|person| self.admit.admits(person, who.email()));
        if !admitted {
            return Err(ReadRefusal::NotFound);
        }
        let id = ShareId::try_from(id).map_err(|_| ReadRefusal::NotFound)?;
        let number = ShareVersion::try_from(version).map_err(|_| ReadRefusal::NotFound)?;
        self.flares
            .shares()
            .html_file(who, &self.files, &id, number)
            .await
            .map_err(|error| match error {
                ShareError::Unavailable => ReadRefusal::Unavailable,
                _ => ReadRefusal::NotFound,
            })
    }
}
