//! The hosted section's share API settings: `stores`, which names the
//! DynamoDB table and the S3 bucket, `creators`, which says who may share,
//! and `names`, which maps usernames to addresses for `--with`. Without
//! `stores` the hosted site serves no share API.
//!
//! `creators` is a list of addresses and `@domain` entries, which selects
//! flare-core's [`CreatorList`], or the value `"anyone admitted"`, which
//! selects [`AllAdmitted`] and lets everyone the `oidc.admit` rule admits
//! share. Left out, it defaults to [`AllAdmitted`] for a `domain` or
//! `emails` rule. An `anyone` rule admits every verified address, so it
//! needs `creators` written out, and the operator chooses `"anyone
//! admitted"` on purpose. An app that embeds flare-server passes its own
//! [`Creators`] to `Flares::new`; the `flare` command offers only these two.
//!
//! `get_flare` is the https address that the reading page's Get Flare link
//! opens for a reader who has never shared a flare. Left out, the reading
//! page shows no link to anyone.

use flare_aws::HostedConfig;
use flare_core::{
    AllAdmitted, CreatorDomain, CreatorList, Creators, CreatorsError, Email, NamedRecipients,
    Principal,
};
use flare_server::GetFlare;
use serde_json::Value;
use std::collections::BTreeMap;

/// The value of `hosted.creators` that lets every admitted person share.
const ANYONE_ADMITTED: &str = "anyone admitted";

/// The creators rule that `hosted.creators` selects when the server starts.
#[derive(Debug)]
pub(crate) enum HostedCreators {
    /// Every admitted person: the key left out, or `"anyone admitted"`.
    All(AllAdmitted),
    /// The listed addresses and domains.
    Listed(CreatorList),
}

impl Creators for HostedCreators {
    async fn may_create(&self, who: &Principal) -> Result<bool, CreatorsError> {
        match self {
            Self::All(all) => all.may_create(who).await,
            Self::Listed(list) => list.may_create(who).await,
        }
    }
}

/// The share API's settings.
#[derive(Debug)]
pub(crate) struct Sharing {
    /// The table and the bucket.
    pub(crate) stores: HostedConfig,
    /// Who may share, responses included.
    pub(crate) creators: HostedCreators,
    /// How `--with` turns an identifier into an address.
    pub(crate) recipients: NamedRecipients,
    /// Whether any account the issuer verifies, which with Google is any
    /// Google account, can sign in and share: `"anyone admitted"` under an
    /// `anyone` admit rule.
    pub(crate) open_to_anyone: bool,
    /// The address the reading page's Get Flare link opens, when set.
    pub(crate) get_flare: Option<GetFlare>,
}

/// The hosted section's share API keys, as the file gave them.
pub(crate) struct Keys {
    /// `hosted.stores`.
    pub(crate) stores: Option<HostedConfig>,
    /// `hosted.creators`.
    pub(crate) creators: Option<Value>,
    /// `hosted.names`.
    pub(crate) names: Option<BTreeMap<String, String>>,
    /// `hosted.get_flare`.
    pub(crate) get_flare: Option<String>,
}

/// The share API's settings from the hosted section's keys, for an admit
/// rule that admits anyone when `anyone` is set.
///
/// # Errors
///
/// Returns the problem, naming the key.
pub(crate) fn sharing(keys: Keys, anyone: bool) -> Result<Option<Sharing>, String> {
    let Keys {
        stores,
        creators,
        names,
        get_flare,
    } = keys;
    let Some(stores) = stores else {
        let orphan = if creators.is_some() {
            "hosted.creators"
        } else if names.is_some() {
            "hosted.names"
        } else if get_flare.is_some() {
            "hosted.get_flare"
        } else {
            return Ok(None);
        };
        return Err(format!(
            "{orphan} needs hosted.stores, the table and bucket that the share API keeps its shares in"
        ));
    };
    let creators = match creators {
        Some(value) => creators_from(&value)?,
        None if anyone => {
            return Err(
                "an admit rule of anyone needs hosted.creators, the addresses that may share, or the value \"anyone admitted\" to let every admitted person share"
                    .to_owned(),
            );
        }
        None => HostedCreators::All(AllAdmitted),
    };
    let recipients = recipients_from(names.unwrap_or_default())?;
    let open_to_anyone = anyone && matches!(creators, HostedCreators::All(_));
    let get_flare = get_flare
        .map(|text| GetFlare::try_from(text.as_str()))
        .transpose()
        .map_err(|_| {
            "hosted.get_flare must be an https URL with a host, such as the project page that readers go to to get Flare".to_owned()
        })?;
    Ok(Some(Sharing {
        stores,
        creators,
        recipients,
        open_to_anyone,
        get_flare,
    }))
}

fn creators_from(value: &Value) -> Result<HostedCreators, String> {
    let bad = || {
        "hosted.creators must be a list of email addresses and @domain entries, such as \"ana@example.com\" and \"@example.com\", or the value \"anyone admitted\""
            .to_owned()
    };
    let list = match value {
        Value::String(word) if word == ANYONE_ADMITTED => {
            return Ok(HostedCreators::All(AllAdmitted));
        }
        Value::Array(list) => list,
        _ => return Err(bad()),
    };
    let (mut addresses, mut domains) = (Vec::new(), Vec::new());
    for entry in list {
        let text = entry.as_str().ok_or_else(bad)?;
        if text.starts_with('@') {
            domains.push(CreatorDomain::try_from(text).map_err(|_| bad())?);
        } else {
            addresses.push(Email::try_from(text).map_err(|_| bad())?);
        }
    }
    Ok(HostedCreators::Listed(CreatorList::listed(
        addresses, domains,
    )))
}

fn recipients_from(names: BTreeMap<String, String>) -> Result<NamedRecipients, String> {
    let bad = "hosted.names must map each username, which is lower-case letters, digits, dots, hyphens and underscores, to an email address";
    let mut pairs = Vec::with_capacity(names.len());
    for (name, address) in names {
        let plain = !name.is_empty()
            && name.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            });
        let email = Email::try_from(address.as_str()).map_err(|_| bad.to_owned())?;
        if !plain {
            return Err(bad.to_owned());
        }
        pairs.push((name, email));
    }
    Ok(NamedRecipients::new(pairs))
}
