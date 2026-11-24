//! The reading page's read of a share, and [`GetFlare`], the address its
//! foot links a reader to when that reader has never shared a flare.

use super::{ReadRefusal, Reading, Sharing};
use flare_core::{
    CommentStore, CursorStore, GrantStore, Principal, ResponseStore, ShareContent, ShareError,
    ShareId, ShareStore, ShareVersion, Standing,
};
use std::fmt;
use url::Url;

/// The deployment's get flare address: an `https` URL with a host and no
/// user name or password. The reading page links a reader who has never
/// shared a flare to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetFlare(String);

/// Why a text is not a get flare address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotGetFlare;

impl fmt::Display for NotGetFlare {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("it is not an https URL with a host")
    }
}

impl std::error::Error for NotGetFlare {}

impl TryFrom<&str> for GetFlare {
    type Error = NotGetFlare;

    /// Admits an absolute `https` URL, written with a lower-case scheme,
    /// whose host is not empty and which names no user. The text kept is
    /// the URL as `url::Url` writes it. The check on the text comes first
    /// because `url::Url` reads `https:///x` as the host `x`.
    fn try_from(text: &str) -> Result<Self, NotGetFlare> {
        let rest = text.strip_prefix("https://").ok_or(NotGetFlare)?;
        if rest.starts_with(['/', '\\']) {
            return Err(NotGetFlare);
        }
        let url = Url::parse(text).map_err(|_| NotGetFlare)?;
        let plain = url.username().is_empty() && url.password().is_none();
        let hosted = url.host_str().is_some_and(|host| !host.is_empty());
        if url.scheme() == "https" && plain && hosted {
            Ok(Self(url.into()))
        } else {
            Err(NotGetFlare)
        }
    }
}

impl GetFlare {
    /// The address, as the link's `href` holds it before escaping.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<S, G, R, C, U, T, X> Sharing<S, G, R, C, U, T, X> {
    /// The share API of `self`, whose reading pages link a reader who has
    /// never shared a flare to `address`.
    #[must_use]
    pub fn with_get_flare(mut self, address: GetFlare) -> Self {
        self.get_flare = Some(address);
        self
    }
}

impl<S, G, R, C, U, T, X> Sharing<S, G, R, C, U, T, X>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
{
    /// The get flare address for `who`, reading a share whose standing is
    /// `standing`: the configured address when `who` owns no share, and
    /// `None` when the site sets none, when `who` is the owner, when `who`
    /// owns a share, or when the store cannot list `who`'s shares.
    async fn badge(&self, who: &Principal, standing: Standing) -> Option<String> {
        let address = self.get_flare.as_ref()?;
        if standing == Standing::Owner {
            return None;
        }
        match self.flares.shares().mine(who).await {
            Ok(owned) if owned.items().is_empty() => Some(address.as_str().to_owned()),
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "the reader's shares could not be listed, so the reading page shows no get flare link"
                );
                None
            }
        }
    }

    pub(super) async fn reading(
        &self,
        who: &Principal,
        id: &str,
        version: Option<&str>,
    ) -> Result<Reading, ReadRefusal> {
        let refused = |error: ShareError| match error {
            ShareError::Unavailable => ReadRefusal::Unavailable,
            _ => ReadRefusal::NotFound,
        };
        let id = ShareId::try_from(id).map_err(|_| ReadRefusal::NotFound)?;
        let number = version
            .map(|text| {
                text.parse::<u32>()
                    .ok()
                    .and_then(|number| ShareVersion::try_from(number).ok())
                    .ok_or(ReadRefusal::NotFound)
            })
            .transpose()?;
        let shares = self.flares.shares();
        let opened = shares.open(who, &id, number).await.map_err(refused)?;
        let ShareContent::Document(text) = opened.version.content() else {
            return Err(ReadRefusal::NotFound);
        };
        let with = match opened.standing {
            Standing::Owner | Standing::Admin => Some(
                shares
                    .grants(who, &id)
                    .await
                    .map_err(refused)?
                    .iter()
                    .map(|grant| grant.grantee().as_str().to_owned())
                    .collect(),
            ),
            Standing::Reader | Standing::Stranger => None,
        };
        Ok(Reading {
            title: opened.share.title().as_str().to_owned(),
            owner: opened.share.owner().as_str().to_owned(),
            own: opened.standing == Standing::Owner,
            with,
            version: opened.version.number().get(),
            published: opened.version.created().as_secs(),
            markdown: text.as_str().to_owned(),
            badge: self.badge(who, opened.standing).await,
        })
    }
}
