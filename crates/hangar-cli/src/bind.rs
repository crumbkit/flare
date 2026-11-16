//! `hangar bind`: saves a binding that fills a page's slots with records,
//! through `hangar_server::ops::bind`.

use crate::board;
use crate::cli::BindArgs;
use crate::codes::Failed;
use crate::documents::Bound;
use crate::env::Snapshot;
use crate::pages;
use hangar_auth_loopback::local_principal;
use hangar_core::{
    BindingName, Fills, PageName, PageStore as _, PageStoreError, ReadError, RecordPath,
    RecordStore as _, SlotName,
};
use hangar_server::ops::{self, BindingError};

/// Runs `hangar bind`. A dry run checks the page, the name and the fit
/// as `save_binding` would, and writes nothing.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn run(args: &BindArgs, dry_run: bool, env: &Snapshot) -> Result<Bound, Failed> {
    let page = PageName::try_from(args.page.as_str()).map_err(|error| Failed::PageInvalid {
        problem: error.to_string(),
    })?;
    let name = BindingName::try_from(args.name.as_str()).map_err(invalid)?;
    let fills = fills(&args.fills)?;
    let (_, store) = board::checked_store(&args.board, env).await?;
    let refused = |error: PageStoreError| pages::refusal(error, &args.page, &args.name);
    if dry_run {
        let latest = store.latest(&page).await.map_err(refused)?;
        match store.binding(&name).await {
            Ok(_) => return Err(refused(PageStoreError::NameTaken)),
            Err(PageStoreError::NotFound) => {}
            Err(other) => return Err(refused(other)),
        }
        check_fit(&store, latest.page(), &fills).await?;
        return Ok(Bound {
            name: args.name.clone(),
            page: args.page.clone(),
            version: latest.version().as_str().to_owned(),
        });
    }
    let made_by = local_principal(&env.user()).map_err(invalid)?;
    let binding = ops::bind(&store, &store, name, &page, fills, made_by)
        .await
        .map_err(|error| match error {
            BindingError::Store(error) => refused(error),
            other => invalid(other),
        })?;
    Ok(Bound {
        name: args.name.clone(),
        page: args.page.clone(),
        version: binding.pin().version().as_str().to_owned(),
    })
}

/// Checks that each record exists and fits its slot of `page`, as
/// `save_binding` does.
async fn check_fit(
    store: &hangar_board::BoardStore,
    page: &hangar_core::Page,
    fills: &Fills,
) -> Result<(), Failed> {
    let paths: Vec<RecordPath> = fills.iter().map(|(_, path)| path.clone()).collect();
    let read = store.read_all(&paths).await;
    let mut types = Vec::new();
    for ((slot, path), record) in fills.iter().zip(read) {
        match record {
            Ok(record) => types.push((slot, record.record_type().clone())),
            Err(ReadError::NotFound) => {
                return Err(Failed::RecordNotFound {
                    path: path.as_str().to_owned(),
                });
            }
            Err(other) => return Err(pages::store_failure_text(other)),
        }
    }
    page.check_fit(types.iter().map(|(slot, found)| (*slot, found)))
        .map_err(|misfit| pages::misfit(&misfit))
}

fn invalid(error: impl std::fmt::Display) -> Failed {
    Failed::BindingInvalid {
        problem: error.to_string(),
    }
}

/// The fills that `SLOT=PATH` words give.
fn fills(words: &[String]) -> Result<Fills, Failed> {
    let mut entries = Vec::new();
    for word in words {
        let Some((slot, path)) = word.split_once('=') else {
            return Err(invalid(format!("{word:?} is not SLOT=PATH")));
        };
        let slot = SlotName::try_from(slot).map_err(invalid)?;
        let path = RecordPath::try_from(path).map_err(invalid)?;
        entries.push((slot, path));
    }
    Fills::new(entries).map_err(invalid)
}
