//! Every code that `flare` declares, gathered into one list.

use super::{Failed, SERVER_NOT_RUNNING, SharingFailed, sharing};
use crumbkit::Code;

const LOCAL: usize = Failed::CODES.len();
const SHARING: usize = SharingFailed::CODES.len();
const API: usize = sharing::API_CODES.len();

/// Every code that `flare` declares: its own, its sharing commands' and
/// the share API's, which a refusal carries.
#[expect(
    clippy::indexing_slicing,
    reason = "each loop's bound keeps its index inside both arrays, and a const block cannot call get"
)]
pub const CODES: &[Code<'static>] = &{
    let mut codes = [SERVER_NOT_RUNNING; LOCAL + SHARING + API];
    let mut index = 0;
    while index < LOCAL {
        codes[index] = Failed::CODES[index];
        index += 1;
    }
    while index < LOCAL + SHARING {
        codes[index] = SharingFailed::CODES[index - LOCAL];
        index += 1;
    }
    while index < LOCAL + SHARING + API {
        codes[index] = sharing::API_CODES[index - LOCAL - SHARING];
        index += 1;
    }
    codes
};
