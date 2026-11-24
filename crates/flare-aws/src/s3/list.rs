//! The bucket's list of stored files, one `ListObjectsV2` page at a time.
//!
//! A page names each object under `files/` with its share, its file id and
//! its `LastModified` time, which S3 sets when the put completes. A key
//! that flare did not write is skipped with a warning, so an object that an
//! operator put in the bucket never stops a sweep and is never deleted by
//! one. The cursor is S3's continuation token.

use crate::dynamodb::error_code;
use crate::dynamodb::items::ItemKind;
use crate::s3::Bucket;
use crate::s3::keys::{FILES_PREFIX, file_of_key};
use aws_sdk_s3::operation::RequestId;
use flare_core::{FileCursor, FilePage, FileStoreError, ListedFile, MAX_FILE_PAGE, Timestamp};

impl Bucket {
    /// One page of at most `limit` files, from the start or from `after`.
    pub(crate) async fn list(
        &self,
        after: Option<&FileCursor>,
        limit: usize,
    ) -> Result<FilePage, FileStoreError> {
        let limit = limit.clamp(1, MAX_FILE_PAGE);
        let request = self
            .client()
            .list_objects_v2()
            .bucket(self.name())
            .prefix(FILES_PREFIX)
            .max_keys(i32::try_from(limit).unwrap_or(1000))
            .set_continuation_token(after.map(|cursor| cursor.as_str().to_owned()));
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
            .map(|token| FileCursor::new(token.to_owned()));
        Ok(FilePage::new(files, next))
    }
}
