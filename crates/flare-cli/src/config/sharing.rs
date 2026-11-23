//! The hosted section's share API settings: `stores`, which names the
//! DynamoDB table and the S3 bucket, and `creators`, which lists who may
//! share. Without `stores` the hosted site serves no share API.
//!
//! `creators` defaults to everyone the `oidc.admit` rule admits. An
//! `anyone` rule admits every verified address, so it needs a `creators`
//! list, or anybody with an account could share from the site.

use flare_aws::HostedConfig;
use flare_core::{Creators, Email};

/// The share API's settings.
#[derive(Debug)]
pub(crate) struct Sharing {
    /// The table and the bucket.
    pub(crate) stores: HostedConfig,
    /// Who may share, responses included.
    pub(crate) creators: Creators,
}

/// The share API's settings from the hosted section's `stores` and
/// `creators`, for an admit rule that admits anyone when `anyone` is set.
///
/// # Errors
///
/// Returns the problem, naming the key.
pub(crate) fn sharing(
    stores: Option<HostedConfig>,
    creators: Option<Vec<String>>,
    anyone: bool,
) -> Result<Option<Sharing>, String> {
    let Some(stores) = stores else {
        return match creators {
            Some(_) => Err(
                "hosted.creators needs hosted.stores, the table and bucket that the share API keeps its shares in"
                    .to_owned(),
            ),
            None => Ok(None),
        };
    };
    let creators = match creators {
        Some(list) => {
            let mut emails = Vec::with_capacity(list.len());
            for text in &list {
                emails.push(Email::try_from(text.as_str()).map_err(|_| {
                    "each entry of hosted.creators must be an email address".to_owned()
                })?);
            }
            Creators::only(emails)
        }
        None if anyone => {
            return Err(
                "an admit rule of anyone needs hosted.creators, the addresses that may share, because it admits every verified address"
                    .to_owned(),
            );
        }
        None => Creators::Everyone,
    };
    Ok(Some(Sharing { stores, creators }))
}
