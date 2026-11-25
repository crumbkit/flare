//! The clause of the [`FileStore`] contract on a list that starts at a
//! share.

use super::FileFixture;
use crate::contract::{Clause, Violation, ensure, unexpected};
use crate::{FileId, FileStore, ListedFile, NewFile, ShareId};

/// Every file the store lists from `start` to the end, read in pages of
/// two, or `None` when the store gives no cursor for `start`.
async fn listed_from<F: FileStore>(
    store: &F,
    start: &ShareId,
) -> Result<Option<Vec<ListedFile>>, Violation> {
    let Some(mut after) = store.cursor_at(start) else {
        return Ok(None);
    };
    let mut listed = Vec::new();
    for _ in 0..super::MAX_PAGES {
        let page = unexpected(store.list(Some(&after), 2).await)?;
        listed.extend(page.files);
        match page.next {
            Some(next) => after = next,
            None => return Ok(Some(listed)),
        }
    }
    Err(Violation::new(
        Clause::AListFromAShareStartsAtThatShare,
        format!("the list did not end within {} pages", super::MAX_PAGES),
    ))
}

/// A list from the cursor at a share names every file of that share and
/// of the shares whose ids sort after it, and none of the shares before.
///
/// # Errors
///
/// Returns [`Clause::AListFromAShareStartsAtThatShare`] otherwise.
pub async fn a_list_from_a_share_starts_at_that_share<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let (before, after) = (&contract_fixture.share, &contract_fixture.other_share);
    let ids: Vec<FileId> = (0x30..0x33_u8)
        .map(|byte| FileId::from_random([byte; 16]))
        .collect();
    for share in [before, after] {
        for id in &ids {
            let file = NewFile::new(
                share.clone(),
                id.clone(),
                contract_fixture.png.clone(),
                contract_fixture.bytes.clone(),
            );
            unexpected(store.put(file).await)?;
        }
    }
    let of = |listed: &[ListedFile], share: &ShareId| {
        listed.iter().filter(|file| file.share() == share).count()
    };
    let from_before = listed_from(store, before).await?;
    let from_after = listed_from(store, after).await?;
    let past_all = listed_from(store, &ShareId::from_random([0xFF; 16])).await?;
    let (Some(from_before), Some(from_after), Some(past_all)) =
        (&from_before, &from_after, &past_all)
    else {
        return Err(Violation::new(
            Clause::AListFromAShareStartsAtThatShare,
            "the store gives no cursor at a share".to_owned(),
        ));
    };
    ensure(
        of(from_before, before) == ids.len()
            && of(from_before, after) == ids.len()
            && from_before.len() == 2 * ids.len()
            && of(from_after, after) == ids.len()
            && from_after.len() == ids.len()
            && past_all.is_empty(),
        Clause::AListFromAShareStartsAtThatShare,
        || {
            format!(
                "from the first share it named {from_before:?}; from the second, {from_after:?}; from past both, {past_all:?}"
            )
        },
    )
}
