//! The [`RecordStore`] clause about reading several paths together.

use super::RecordFixture;
use crate::RecordStore;
use crate::contract::{Clause, Violation, ensure};

/// Reading the fixture's record, an unknown path and the record again with
/// [`RecordStore::read_all`] answers three results, in that order, each
/// equal to what [`RecordStore::read`] answers for its path.
///
/// # Errors
///
/// Returns [`Clause::ReadAllAnswersEachPathAsReadDoes`] otherwise.
pub async fn read_all_answers_each_path_as_read_does<S: RecordStore>(
    store: &S,
    contract_fixture: &RecordFixture,
) -> Result<(), Violation> {
    let target = contract_fixture.target()?.path().clone();
    let paths = [
        target.clone(),
        contract_fixture.missing.clone(),
        target.clone(),
    ];
    let together = store.read_all(&paths).await;
    let mut alone = Vec::new();
    for path in &paths {
        alone.push(store.read(path).await);
    }
    ensure(
        together == alone && together.first().is_some_and(Result::is_ok),
        Clause::ReadAllAnswersEachPathAsReadDoes,
        || format!("read_all answered {together:?}, read answered {alone:?}"),
    )
}
