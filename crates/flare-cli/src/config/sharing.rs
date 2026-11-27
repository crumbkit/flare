//! The hosted section's share API settings: `stores`, which names the
//! DynamoDB table and the S3 bucket, `creators`, which says who may share,
//! `names`, which maps names to people for `--with`, and
//! `everyone_signed_in`, which lets an everyone grant under an `anyone`
//! rule open to every signed-in person, and `teams`, which config/teams.rs
//! reads. Without `stores` the hosted site
//! serves no share API. Under the username kind, `names` and `creators`
//! hold usernames where the email kind holds addresses.
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
//!
//! `content_origin` is the https origin of the content host, which serves
//! the HTML file of an uploaded HTML share to a frame of the reading page.
//! It must differ from `hosted.origin`, so the uploaded page's scripts run
//! in an origin of their own. Left out, the server refuses HTML uploads.

use super::teams::{TeamsSource, teams as teams_from};
use flare_aws::HostedConfig;
use flare_core::{
    AllAdmitted, CreatorDomain, CreatorList, Creators, CreatorsError, IdentityKind,
    NamedRecipients, Principal,
};
use flare_server::{GetFlare, PublicOrigin};
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
    /// How `--with` turns an identifier into a person.
    pub(crate) recipients: NamedRecipients,
    /// Whether an everyone grant under an `anyone` rule opens to every
    /// signed-in person.
    pub(crate) everyone_signed_in: bool,
    /// Whether any account the issuer verifies, which with Google is any
    /// Google account, can sign in and share: `"anyone admitted"` under an
    /// `anyone` admit rule.
    pub(crate) open_to_anyone: bool,
    /// The address the reading page's Get Flare link opens, when set.
    pub(crate) get_flare: Option<GetFlare>,
    /// The origin of the content host, when set.
    pub(crate) content_origin: Option<PublicOrigin>,
    /// Where the teams come from.
    pub(crate) teams: TeamsSource,
}

/// The hosted section's share API keys, as the file gave them.
pub(crate) struct Keys<'a> {
    /// `hosted.stores`.
    pub(crate) stores: Option<HostedConfig>,
    /// `hosted.creators`.
    pub(crate) creators: Option<Value>,
    /// `hosted.names`.
    pub(crate) names: Option<BTreeMap<String, String>>,
    /// `hosted.get_flare`.
    pub(crate) get_flare: Option<String>,
    /// `hosted.content_origin`.
    pub(crate) content_origin: Option<String>,
    /// `hosted.origin`, which the content origin must differ from.
    pub(crate) origin: &'a PublicOrigin,
    /// `hosted.everyone_signed_in`.
    pub(crate) everyone_signed_in: Option<bool>,
    /// `hosted.teams`.
    pub(crate) teams: Option<Value>,
}

/// The share API's settings from the hosted section's keys, for an admit
/// rule that admits anyone when `anyone` is set, naming people of `kind`.
///
/// # Errors
///
/// Returns the problem, naming the key.
pub(crate) fn sharing(
    keys: Keys<'_>,
    anyone: bool,
    kind: IdentityKind,
) -> Result<Option<Sharing>, String> {
    let Keys {
        stores,
        creators,
        names,
        get_flare,
        content_origin,
        origin,
        everyone_signed_in,
        teams,
    } = keys;
    let Some(stores) = stores else {
        let orphan = if creators.is_some() {
            "hosted.creators"
        } else if names.is_some() {
            "hosted.names"
        } else if get_flare.is_some() {
            "hosted.get_flare"
        } else if content_origin.is_some() {
            "hosted.content_origin"
        } else if everyone_signed_in.is_some() {
            "hosted.everyone_signed_in"
        } else if teams.is_some() {
            "hosted.teams"
        } else {
            return Ok(None);
        };
        return Err(format!(
            "{orphan} needs hosted.stores, the table and bucket that the share API keeps its shares in"
        ));
    };
    let creators = match creators {
        Some(value) => creators_from(&value, kind)?,
        None if anyone => {
            return Err(
                "an admit rule of anyone needs hosted.creators, the addresses that may share, or the value \"anyone admitted\" to let every admitted person share"
                    .to_owned(),
            );
        }
        None => HostedCreators::All(AllAdmitted),
    };
    let recipients = recipients_from(names.unwrap_or_default(), kind)?;
    let teams = teams_from(teams.as_ref())?;
    let open_to_anyone = anyone && matches!(creators, HostedCreators::All(_));
    let get_flare = get_flare
        .map(|text| GetFlare::try_from(text.as_str()))
        .transpose()
        .map_err(|_| {
            "hosted.get_flare must be an https URL with a host, such as the project page that readers go to to get Flare".to_owned()
        })?;
    let content_origin = content_origin
        .map(|text| content_origin_from(&text, origin))
        .transpose()?;
    Ok(Some(Sharing {
        stores,
        creators,
        recipients,
        everyone_signed_in: everyone_signed_in.unwrap_or(false),
        open_to_anyone,
        get_flare,
        content_origin,
        teams,
    }))
}

/// The content origin that `text` names, which must be an https origin on
/// a separate domain from the site's `origin`.
fn content_origin_from(text: &str, origin: &PublicOrigin) -> Result<PublicOrigin, String> {
    let content = PublicOrigin::try_from(text).map_err(|_| {
        "hosted.content_origin must be an https origin with a host, such as https://usercontent.example.org".to_owned()
    })?;
    if content.is_separate_domain_from(origin) {
        Ok(content)
    } else {
        Err("hosted.content_origin must be on a separate domain from hosted.origin: its host may not be hosted.origin's host on any port, a name under it, or a name it is under".to_owned())
    }
}

fn creators_from(value: &Value, kind: IdentityKind) -> Result<HostedCreators, String> {
    let bad = || {
        format!(
            "hosted.creators must be a list of people, each {}, and @domain entries, such as \"@example.com\", or the value \"anyone admitted\"",
            person_shape(kind)
        )
    };
    let list = match value {
        Value::String(word) if word == ANYONE_ADMITTED => {
            return Ok(HostedCreators::All(AllAdmitted));
        }
        Value::Array(list) => list,
        _ => return Err(bad()),
    };
    let (mut people, mut domains) = (Vec::new(), Vec::new());
    for entry in list {
        let text = entry.as_str().ok_or_else(bad)?;
        if text.starts_with('@') {
            domains.push(CreatorDomain::try_from(text).map_err(|_| bad())?);
        } else {
            people.push(kind.person(text).map_err(|_| bad())?);
        }
    }
    Ok(HostedCreators::Listed(CreatorList::listed(people, domains)))
}

/// How a person of `kind` is written, for a refusal.
fn person_shape(kind: IdentityKind) -> &'static str {
    match kind {
        IdentityKind::Email => "an email address such as \"ana@example.com\"",
        IdentityKind::Username => "a username such as \"ana\"",
    }
}

fn recipients_from(
    names: BTreeMap<String, String>,
    kind: IdentityKind,
) -> Result<NamedRecipients, String> {
    let bad = format!(
        "hosted.names must map each name, which is lower-case letters, digits, dots, hyphens and underscores, to a person, each {}",
        person_shape(kind)
    );
    let mut pairs = Vec::with_capacity(names.len());
    for (name, address) in names {
        let plain = !name.is_empty()
            && name.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            });
        let person = kind.person(address.as_str()).map_err(|_| bad.clone())?;
        if !plain {
            return Err(bad);
        }
        pairs.push((name, person));
    }
    Ok(NamedRecipients::new(pairs).with_identity(kind))
}
