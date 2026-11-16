//! The failures that a page store's refusals become.

use crate::codes::Failed;
use hangar_core::{Misfit, PageStoreError};

/// The failure for a page store's refusal. `name` is the page or binding
/// the command named, for `NotFound`.
pub(crate) fn refusal(error: PageStoreError, page: &str, binding: &str) -> Failed {
    match error {
        PageStoreError::NotFound => Failed::PageNotFound {
            name: page.to_owned(),
        },
        PageStoreError::NameTaken => Failed::NameTaken {
            name: binding.to_owned(),
        },
        PageStoreError::Misfit(misfit) => self::misfit(&misfit),
        other => store_failure_text(other),
    }
}

/// The failure for a record that does not fit its slot.
pub(crate) fn misfit(misfit: &Misfit) -> Failed {
    match misfit {
        Misfit::MissingRecord { path, .. } => Failed::RecordNotFound {
            path: path.as_str().to_owned(),
        },
        other => Failed::Misfit {
            problem: other.to_string(),
        },
    }
}

/// The failure for a store that could not answer, with its message.
pub(crate) fn store_failure_text(error: impl std::fmt::Display) -> Failed {
    Failed::BoardUnavailable {
        problem: error.to_string(),
    }
}
