//! The bucket's list of stored files, one `ListObjectsV2` page at a time.
//!
//! A page names each object under `files/` with its share, its file id and
//! its `LastModified` time, which S3 sets when the put completes. A key
//! that flare did not write is skipped with a warning, so an object that an
//! operator put in the bucket never stops a sweep and is never deleted by
//! one. A cursor is either S3's continuation token, written `token:<token>`,
//! or a share to start at, written `after:<key>`, which the list sends as
//! `StartAfter`. The key `files/<share id>` sorts before every key of that
//! share and after every key of a share whose id sorts before it.

use crate::dynamodb::error_code;
use crate::dynamodb::items::ItemKind;
use crate::s3::Bucket;
use crate::s3::keys::{FILES_PREFIX, file_of_key};
use aws_sdk_s3::operation::RequestId;
use flare_core::{
    FileCursor, FilePage, FileStoreError, ListedFile, MAX_FILE_PAGE, ShareId, Timestamp,
};

impl Bucket {
    /// One page of at most `limit` files, from the start or from `after`.
    pub(crate) async fn list(
        &self,
        after: Option<&FileCursor>,
        limit: usize,
    ) -> Result<FilePage, FileStoreError> {
        let limit = limit.clamp(1, MAX_FILE_PAGE);
        let (token, start_after) = match after.map(FileCursor::as_str) {
            None => (None, None),
            Some(text) => match (text.strip_prefix(TOKEN), text.strip_prefix(AFTER)) {
                (Some(token), _) => (Some(token.to_owned()), None),
                (None, Some(key)) => (None, Some(key.to_owned())),
                (None, None) => return Err(FileStoreError::Unavailable),
            },
        };
        let request = self
            .client()
            .list_objects_v2()
            .bucket(self.name())
            .prefix(FILES_PREFIX)
            .max_keys(i32::try_from(limit).unwrap_or(1000))
            .set_continuation_token(token)
            .set_start_after(start_after);
        let output = match request.send().await {
            Ok(output) => output,
            Err(error) => {
                tracing::warn!(
                    call = "ListObjectsV2",
                    item = ItemKind::Object.name(),
                    code = error_code(&error),
                    request_id = error.request_id().unwrap_or("none"),
                    "an S3 call failed"
                );
                return Err(FileStoreError::Unavailable);
            }
        };
        let mut files = Vec::new();
        for object in output.contents() {
            let key = object.key().unwrap_or_default();
            let Some((share, id)) = file_of_key(key) else {
                tracing::warn!(
                    item = ItemKind::Object.name(),
                    "the bucket holds an object under files/ that flare did not write; the list skips it"
                );
                continue;
            };
            let stored = object
                .last_modified()
                .and_then(|time| u64::try_from(time.secs()).ok())
                .ok_or(FileStoreError::Unavailable)?;
            files.push(ListedFile::new(share, id, Timestamp::from_secs(stored)));
        }
        let next = output
            .is_truncated()
            .unwrap_or(false)
            .then(|| output.next_continuation_token())
            .flatten()
            .map(|token| FileCursor::new(format!("{TOKEN}{token}")));
        Ok(FilePage::new(files, next))
    }
}

/// The prefix of a cursor that holds S3's continuation token.
const TOKEN: &str = "token:";

/// The prefix of a cursor that holds a key to list after.
const AFTER: &str = "after:";

/// The cursor from which a list names the files of `share` and of every
/// share after it.
pub(crate) fn cursor_at(share: &ShareId) -> FileCursor {
    FileCursor::new(format!("{AFTER}{FILES_PREFIX}{}", share.as_str()))
}
