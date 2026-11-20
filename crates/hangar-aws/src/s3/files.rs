//! [`S3FileStore`], hangar-core's `FileStore` on the bucket.
//!
//! A put creates its object only when the key is free, with
//! `If-None-Match: *`, so a taken id leaves the first object. A put whose
//! answer is lost is sent again, and a refusal of the resend for a taken
//! key is success when the stored object equals the file. A read answers
//! not found only for a missing key; any other failure, including a refused
//! request, is unavailable, so a policy mistake never reads as a missing
//! file. The read stops at [`MAX_FILE_BYTES`] while the body streams.

use crate::dynamodb::Faults;
use crate::dynamodb::error_code;
use crate::dynamodb::items::ItemKind;
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::s3::Bucket;
use crate::s3::keys::object_key;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::RequestId;
use aws_sdk_s3::operation::put_object::builders::PutObjectFluentBuilder;
use aws_sdk_s3::primitives::ByteStream;
use hangar_core::{
    FileBytes, FileId, FileStore, FileStoreError, MAX_FILE_BYTES, MediaType, NewFile, ShareId,
    StoredFile,
};

/// Keeps the files of shares in the bucket, each under
/// `files/<share id>/<file id>`.
#[derive(Debug, Clone)]
pub struct S3FileStore {
    bucket: Bucket,
}

impl S3FileStore {
    pub(crate) fn from_bucket(bucket: Bucket) -> Self {
        Self { bucket }
    }
}

/// What a failed put means, from the HTTP status of its answer, or `None`
/// when no answer came.
pub(crate) fn put_failure(status: Option<u16>) -> Sent<(), FileStoreError> {
    match status {
        Some(412) => Sent::Refused(FileStoreError::IdTaken),
        // Another put of the same key is in flight; its retry meets the 412.
        Some(409) => Sent::Again,
        None => Sent::Unknown,
        Some(status) if status >= 500 => Sent::Unknown,
        Some(_) => Sent::Unavailable,
    }
}

/// What a failed read means: not found only for a missing key.
pub(crate) fn get_failure(status: Option<u16>, no_such_key: bool) -> FileStoreError {
    if no_such_key && status == Some(404) {
        FileStoreError::NotFound
    } else {
        FileStoreError::Unavailable
    }
}

fn log_failure(call: &'static str, code: &str, request_id: Option<&str>) {
    tracing::warn!(
        call,
        item = ItemKind::Object.name(),
        code,
        request_id = request_id.unwrap_or("none"),
        "an S3 call failed"
    );
}

impl Bucket {
    /// The create-only `PutObject` of `file`.
    pub(crate) fn put_request(&self, file: &NewFile) -> PutObjectFluentBuilder {
        self.client()
            .put_object()
            .bucket(self.name())
            .key(object_key(file.share(), file.id()))
            .if_none_match("*")
            .content_type(file.media_type().as_str())
            .body(ByteStream::from(file.bytes().as_bytes().to_vec()))
    }

    /// Reads the file `id` of `share`.
    async fn read(&self, share: &ShareId, id: &FileId) -> Result<StoredFile, FileStoreError> {
        let read = self
            .client()
            .get_object()
            .bucket(self.name())
            .key(object_key(share, id))
            .send()
            .await;
        let output = match read {
            Ok(output) => output,
            Err(error) => {
                let no_such_key = error
                    .as_service_error()
                    .is_some_and(aws_sdk_s3::operation::get_object::GetObjectError::is_no_such_key);
                let status = error.raw_response().map(|raw| raw.status().as_u16());
                let failure = get_failure(status, no_such_key);
                if failure == FileStoreError::Unavailable {
                    log_failure("GetObject", &error_code(&error), error.request_id());
                }
                return Err(failure);
            }
        };
        let declared = output.content_length().unwrap_or(0);
        if usize::try_from(declared).map_or(true, |length| length > MAX_FILE_BYTES) {
            log_failure("GetObject", "ObjectTooLarge", None);
            return Err(FileStoreError::Unavailable);
        }
        let Some(media_type) = output
            .content_type()
            .and_then(|media| MediaType::try_from(media).ok())
        else {
            log_failure("GetObject", "UnreadableContentType", None);
            return Err(FileStoreError::Unavailable);
        };
        let mut body = output.body;
        let mut bytes = Vec::new();
        loop {
            match body.try_next().await {
                Ok(Some(chunk)) => {
                    if bytes.len().saturating_add(chunk.len()) > MAX_FILE_BYTES {
                        log_failure("GetObject", "ObjectTooLarge", None);
                        return Err(FileStoreError::Unavailable);
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(_) => {
                    log_failure("GetObject", "BodyReadFailed", None);
                    return Err(FileStoreError::Unavailable);
                }
            }
        }
        let bytes = FileBytes::try_from(bytes).map_err(|_| FileStoreError::Unavailable)?;
        Ok(StoredFile::from(NewFile::new(
            share.clone(),
            id.clone(),
            media_type,
            bytes,
        )))
    }
}

/// A put of one file.
struct PutOp<'a> {
    bucket: &'a Bucket,
    file: &'a NewFile,
}

impl Attempt for PutOp<'_> {
    type Built = ();
    type Done = ();
    type Refused = FileStoreError;

    async fn build(&self) -> Result<(), Sent<(), FileStoreError>> {
        Ok(())
    }

    /// S3 has no request token; a resend is a new create-only put, which a
    /// stored object refuses with 412 and the loop settles.
    async fn send(&self, (): &(), _token: &Token) -> Sent<(), FileStoreError> {
        let faults = self.bucket.faults();
        if faults == Faults::DropRequests {
            return Sent::Unknown;
        }
        match self.bucket.put_request(self.file).send().await {
            Ok(_) if faults == Faults::LoseAnswers => Sent::Unknown,
            Ok(_) => Sent::Done(()),
            Err(SdkError::ConstructionFailure(_)) => Sent::Unavailable,
            Err(error) => {
                let status = error.raw_response().map(|raw| raw.status().as_u16());
                let sent = put_failure(status);
                if sent == Sent::Unavailable {
                    log_failure("PutObject", &error_code(&error), error.request_id());
                }
                sent
            }
        }
    }

    async fn settle(&self, (): &()) -> Settled<()> {
        match self.bucket.read(self.file.share(), self.file.id()).await {
            Ok(stored) if stored == StoredFile::from(self.file.clone()) => Settled::Matches(()),
            Ok(_) | Err(FileStoreError::NotFound) => Settled::Differs,
            Err(_) => Settled::Unavailable,
        }
    }
}

impl FileStore for S3FileStore {
    async fn put(&self, file: NewFile) -> Result<(), FileStoreError> {
        let op = PutOp {
            bucket: &self.bucket,
            file: &file,
        };
        match run(self.bucket.policy(), &op).await {
            Final::Done(()) => Ok(()),
            Final::Refused(refused) => Err(refused),
            Final::Unavailable => Err(FileStoreError::Unavailable),
        }
    }

    async fn get(&self, share: &ShareId, id: &FileId) -> Result<StoredFile, FileStoreError> {
        self.bucket.read(share, id).await
    }
}

#[cfg(test)]
mod tests;
