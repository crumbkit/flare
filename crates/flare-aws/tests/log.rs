//! A failing call answers unavailable, never not found or an empty list,
//! and its log line holds the operation and the error code and no email,
//! key, title or text.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use flare_aws::Stores;
use flare_core::{
    DocumentText, Email, GrantStore, GrantStoreError, Principal, Role, ShareContent, ShareError,
    ShareId, ShareStore, ShareStoreError, ShareTitle, Shares, Timestamp,
};
use stand_in::{endpoints, unmade_config};
use std::io::Write;
use std::sync::{Arc, Mutex};

/// A writer that keeps every log line in memory.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Self;

    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

#[tokio::test]
async fn a_failing_call_logs_its_operation_and_code_and_no_personal_data() {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    // The table does not exist, so every call fails.
    let stores = Stores::connect_at(&unmade_config(), &endpoints());
    let id = ShareId::from_random([0x5e; 16]);
    let owner = Email::try_from("private.owner@example.com").unwrap();
    let who = Principal::with_email(owner.principal_id(), owner.clone());
    let grantee = Email::try_from("private.grantee@example.com").unwrap();

    let listed = stores.shares.owned_by(&owner.principal_id()).await;
    assert_eq!(listed, Err(ShareStoreError::Unavailable));
    let got = stores.shares.get(&id).await;
    assert_eq!(got, Err(ShareStoreError::Unavailable));
    let granted_to = stores.grants.granted_to(&grantee.principal_id()).await;
    assert_eq!(granted_to, Err(GrantStoreError::Unavailable));
    let of_share = stores.grants.of_share(&id).await;
    assert_eq!(of_share, Err(GrantStoreError::Unavailable));

    let title = ShareTitle::try_from("Private title of a person").unwrap();
    let text = DocumentText::try_from("Private document text").unwrap();
    let shares = Shares::new(stores.shares, stores.grants);
    let created = shares
        .create(
            &who,
            id.clone(),
            title,
            ShareContent::Document(text),
            Timestamp::from_secs(1),
        )
        .await;
    assert_eq!(created, Err(ShareError::Unavailable));
    let granted = shares
        .grant(&who, &id, &grantee, Role::Reader, Timestamp::from_secs(2))
        .await;
    assert_eq!(granted, Err(ShareError::Unavailable));

    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(log.contains("ResourceNotFoundException"), "{log}");
    assert!(log.contains("Query"), "{log}");
    assert!(log.contains("GetItem"), "{log}");
    assert!(log.contains("TransactWriteItems"), "{log}");
    for private in [
        "private.owner",
        "private.grantee",
        "example.com",
        "Private title",
        "Private document",
        id.as_str(),
        "SHARE#",
        "OWN#",
        "TO#",
    ] {
        assert!(!log.contains(private), "the log holds {private:?}: {log}");
    }
}
