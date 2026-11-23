//! The [`PageStore`] clause about reading a binding's fills at one state of
//! the records.

use super::{PageFixture, binding, binding_name, is_not_found, page_name, pin, publish};
use crate::contract::{Clause, Violation, ensure, fixture};
use crate::{
    Blob, PageStore, ReadError, Record, RecordBody, RecordPath, RecordStore, RecordType,
    WriteError, WriteProvenance,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The types that one state of the records gives each path.
type State = HashMap<RecordPath, RecordType>;

/// A record store that the suite owns. [`RecordStore::read_all`] answers
/// every path from the first state. A single [`RecordStore::read`] answers
/// from the first state and the next one from the second, in turn, as a
/// store would if another writer changed the records between two reads.
struct TwoStates {
    first: State,
    second: State,
    reads: AtomicUsize,
}

impl TwoStates {
    fn answer(state: &State, path: &RecordPath) -> Result<Record, ReadError> {
        let record_type = state.get(path).ok_or(ReadError::NotFound)?;
        Ok(Record::new(
            path.clone(),
            record_type.clone(),
            Blob::try_from("b".repeat(40).as_str()).map_err(|_| ReadError::Unavailable)?,
            RecordBody::try_from("").map_err(|_| ReadError::Unavailable)?,
        ))
    }
}

impl RecordStore for TwoStates {
    async fn read(&self, path: &RecordPath) -> Result<Record, ReadError> {
        let turn = self.reads.fetch_add(1, Ordering::SeqCst);
        let state = if turn.is_multiple_of(2) {
            &self.first
        } else {
            &self.second
        };
        Self::answer(state, path)
    }

    async fn read_all(&self, paths: &[RecordPath]) -> Vec<Result<Record, ReadError>> {
        paths
            .iter()
            .map(|path| Self::answer(&self.first, path))
            .collect()
    }

    async fn write(
        &self,
        _path: &RecordPath,
        _body: &RecordBody,
        _provenance: &WriteProvenance,
    ) -> Result<Blob, WriteError> {
        Err(WriteError::Unavailable)
    }
}

/// A binding that fills `notes` with `/notes/n` and `tasks` with `/tasks/a`
/// is checked against records that change between two reads. In the first
/// state both records are notes, so `tasks` misfits. In the second both are
/// tasks, so `notes` misfits. A store that reads the `notes` fill in the
/// first state and the `tasks` fill in the second sees a fit. A store that
/// reads both fills at one state, through [`RecordStore::read_all`], refuses
/// the binding with `Misfit` and saves nothing.
///
/// # Errors
///
/// Returns [`Clause::ATwoFillBindingIsCheckedAgainstOneState`] otherwise.
pub async fn a_two_fill_binding_is_checked_against_one_state<S: PageStore, R: RecordStore>(
    store: &S,
    _records: &R,
    contract_fixture: &PageFixture,
) -> Result<(), Violation> {
    let task = fixture(RecordType::try_from(super::TASK_TYPE))?;
    let note = fixture(RecordType::try_from(super::NOTE_TYPE))?;
    let task_a = fixture(RecordPath::try_from(super::TASK_A))?;
    let note_n = fixture(RecordPath::try_from(super::NOTE))?;
    let records = TwoStates {
        first: HashMap::from([(task_a.clone(), note.clone()), (note_n.clone(), note)]),
        second: HashMap::from([(task_a, task.clone()), (note_n, task)]),
        reads: AtomicUsize::new(0),
    };
    let v1 = publish(
        store,
        contract_fixture,
        "home",
        "one",
        super::TASKS_AND_NOTES,
    )
    .await?;
    let mixed = binding(
        "mixed-view",
        pin("home", v1.as_str())?,
        &[("tasks", super::TASK_A), ("notes", super::NOTE)],
        contract_fixture.original()?,
    )?;
    let answer = store.save_binding(mixed, &records).await;
    let stored = store.binding(&binding_name("mixed-view")?).await;
    let listed = store.bindings_of(&page_name("home")?).await;
    ensure(
        matches!(answer, Err(crate::PageStoreError::Misfit(_)))
            && is_not_found(&stored)
            && listed.as_ref().is_ok_and(Vec::is_empty),
        Clause::ATwoFillBindingIsCheckedAgainstOneState,
        || format!("the save answered {answer:?}, stored {stored:?}, listed {listed:?}"),
    )
}
