//! The [`Shares`] methods for shares whose versions show images: create
//! and publish with an image table, and read a file the table lists.

use super::Shares;
use crate::auth::Principal;
use crate::file::{FileId, StoredFile, VersionImages};
use crate::share::{NewShare, NewVersion, Share, ShareContent, ShareId, ShareTitle, ShareVersion};
use crate::sharing::ShareAction;
use crate::sharing::error::ShareError;
use crate::store::{FileStore, GrantStore, ShareStore};
use crate::teams::Teams;
use crate::time::Timestamp;

impl<S: ShareStore, G: GrantStore, T: Teams> Shares<S, G, T> {
    /// Creates a share as [`Shares::create`] does, whose version 1 shows
    /// the images in `images`. The caller has stored each file under `id`
    /// before it calls this.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Shares::create`].
    pub async fn create_with_images(
        &self,
        who: &Principal,
        id: ShareId,
        title: ShareTitle,
        content: ShareContent,
        images: VersionImages,
        at: Timestamp,
    ) -> Result<Share, ShareError> {
        let owner = self.actor(who)?.id;
        let new = NewShare::new(id, owner, title, content, at).with_images(images);
        Ok(self.shares.create(new).await?)
    }

    /// Publishes a version as [`Shares::publish`] does, showing the images
    /// in `images`. The caller has stored each file under `id` before it
    /// calls this.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Shares::publish`].
    pub async fn publish_with_images(
        &self,
        who: &Principal,
        id: &ShareId,
        content: ShareContent,
        images: VersionImages,
        at: Timestamp,
    ) -> Result<ShareVersion, ShareError> {
        let actor = self.actor(who)?;
        let (share, _) = self.admit(&actor, id, ShareAction::Publish).await?;
        if share.kind() != content.kind() {
            return Err(ShareError::KindChanged);
        }
        let version = NewVersion::new(content, actor.id, at).with_images(images);
        Ok(self.shares.publish(id, version).await?)
    }

    /// Reads a file that version `number` of a share lists in its image
    /// table, for a caller who may read the share. A file stored under the
    /// share that the version does not list is never read, so a file that a
    /// failed create or publish left behind is never served.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`] for a missing share, a caller with
    /// no standing in it, a missing version, a file the version does not
    /// list and a listed file the store does not hold, so the answers
    /// cannot be told apart; [`ShareError::NoPerson`] and
    /// [`ShareError::Unavailable`].
    pub async fn file<F: FileStore>(
        &self,
        who: &Principal,
        files: &F,
        id: &ShareId,
        number: ShareVersion,
        file: &FileId,
    ) -> Result<StoredFile, ShareError> {
        let opened = self
            .open(who, id, Some(number))
            .await
            .map_err(|error| match error {
                ShareError::VersionNotFound => ShareError::NotFound,
                other => other,
            })?;
        if !opened.version.images().names(file) {
            return Err(ShareError::NotFound);
        }
        Ok(files.get(id, file).await?)
    }
    /// Reads the HTML file of version `number` of a share, for a caller who
    /// may read the share, together with the share. A version that holds
    /// Markdown has no file.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`] for a missing share, a caller with
    /// no standing in it, a missing version, a version that holds no HTML
    /// and a listed file the store does not hold, so the answers cannot be
    /// told apart; [`ShareError::NoPerson`] and [`ShareError::Unavailable`].
    pub async fn html_file<F: FileStore>(
        &self,
        who: &Principal,
        files: &F,
        id: &ShareId,
        number: ShareVersion,
    ) -> Result<StoredFile, ShareError> {
        let opened = self
            .open(who, id, Some(number))
            .await
            .map_err(|error| match error {
                ShareError::VersionNotFound => ShareError::NotFound,
                other => other,
            })?;
        let ShareContent::Html(html) = opened.version.content() else {
            return Err(ShareError::NotFound);
        };
        Ok(files.get(id, html.file()).await?)
    }
}
