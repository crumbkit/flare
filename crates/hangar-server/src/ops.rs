//! The two writes that need no server: publishing a page and saving a
//! binding. `hangar publish` and `hangar bind` call them on a board.

use hangar_core::{
    Binding, BindingName, Fills, MadeFrom, Page, PageName, PagePin, PageStore, PageStoreError,
    PageVersion, Principal, Provenance, RecordStore,
};
use hangar_render::TemplateFlaws;

/// Why a page was not published.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PublishError {
    /// The page's template has flaws; nothing was written.
    #[error("the page's template has flaws")]
    Flaws(#[source] TemplateFlaws),
    /// The store refused or could not answer.
    #[error("the page could not be published")]
    Store(#[source] PageStoreError),
}

/// Why a binding was not saved.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BindingError {
    /// The provenance could not be built.
    #[error("the binding's provenance could not be built")]
    Provenance(#[source] hangar_core::ParseError),
    /// The store refused or could not answer.
    #[error("the binding could not be saved")]
    Store(#[source] PageStoreError),
}

/// Checks `page` with `hangar_render::check_page` and publishes it.
///
/// # Errors
///
/// Returns [`PublishError::Flaws`] before any write when the template has
/// flaws, and [`PublishError::Store`] when the store refuses.
pub async fn publish(
    pages: &impl PageStore,
    page: Page,
    provenance: Provenance,
) -> Result<PageVersion, PublishError> {
    hangar_render::check_page(&page).map_err(PublishError::Flaws)?;
    pages
        .publish(page, provenance)
        .await
        .map_err(PublishError::Store)
}

/// Pins the newest version of `page` and saves the binding `name` with
/// `fills`, made by `made_by` from that version.
///
/// # Errors
///
/// Returns [`BindingError::Store`] for the store's refusals, such as
/// `NotFound` for an unknown page, `NameTaken` and `Misfit`.
pub async fn bind(
    pages: &impl PageStore,
    records: &impl RecordStore,
    name: BindingName,
    page: &PageName,
    fills: Fills,
    made_by: Principal,
) -> Result<Binding, BindingError> {
    let newest = pages.latest(page).await.map_err(BindingError::Store)?;
    let pin = PagePin::new(page.clone(), newest.version().clone());
    let provenance = Provenance::new(made_by, MadeFrom::Page(pin.clone()), Vec::new())
        .map_err(BindingError::Provenance)?;
    let binding = Binding::new(name, pin, fills, provenance);
    pages
        .save_binding(binding.clone(), records)
        .await
        .map_err(BindingError::Store)?;
    Ok(binding)
}
