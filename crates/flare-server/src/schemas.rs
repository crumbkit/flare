//! [`Schemas`]: where the server reads a record type's JSON Schema.

use flare_board::BoardStore;
use flare_core::{ReadError, RecordType};
use std::future::Future;

/// Reads the JSON Schema text of a record type, which the edit form and
/// the post check need. `BoardStore::schema` is an inherent method and the
/// server takes its stores as generics, so the server names this trait and
/// implements it for [`BoardStore`].
pub trait Schemas: Send + Sync {
    /// The schema of `record_type`, as JSON Schema 2020-12 text.
    ///
    /// # Errors
    ///
    /// Returns [`ReadError::NotFound`] when there is no schema for the type
    /// and [`ReadError::Unavailable`] when the store cannot answer.
    fn schema(
        &self,
        record_type: &RecordType,
    ) -> impl Future<Output = Result<String, ReadError>> + Send;
}

impl Schemas for BoardStore {
    fn schema(
        &self,
        record_type: &RecordType,
    ) -> impl Future<Output = Result<String, ReadError>> + Send {
        BoardStore::schema(self, record_type)
    }
}
