use super::token::{self, Claims, LIFETIME, Refused};
use super::{ContentHost, ViewFailure};
use crate::site::PublicOrigin;
use flare_core::memory::MemorySealingKeys;
use flare_core::{Email, KeyBytes, KeyId, Person, Principal, PrincipalId, SealingKey, Timestamp};

fn key(id: u32, byte: u8) -> SealingKey {
    SealingKey::new(
        KeyId::new(id).unwrap(),
        KeyBytes::new([byte; 32]),
        Timestamp::from_secs(0),
    )
}

fn viewer() -> Principal {
    let address = Email::try_from("bo@example.com").unwrap();
    Principal::with_person(
        PrincipalId::try_from("bo@example.com").unwrap(),
        Person::Email(address.clone()),
        Some(address),
    )
}

fn claims() -> Claims {
    Claims {
        share: "0123456789abcdef0123456789abcdef".to_owned(),
        version: 2,
        viewer: viewer(),
    }
}

fn host() -> ContentHost {
    let origin = PublicOrigin::try_from("https://content.example.com").unwrap();
    ContentHost::new(origin, MemorySealingKeys::new())
}

/// The text after the key id of a minted token.
fn body(text: &str) -> &str {
    token::split(text).unwrap().1
}

#[test]
fn a_token_opens_to_the_claims_it_sealed_under_the_same_key() {
    let key = key(1, 7);
    let text = token::seal(&key, &claims(), 1_000).unwrap();
    let (id, rest) = token::split(&text).unwrap();
    assert_eq!(id, 1);
    assert_eq!(token::open(&key, rest, 1_000), Ok(claims()));
    assert_eq!(token::open(&key, rest, 1_000 + LIFETIME - 1), Ok(claims()));
}

#[test]
fn a_token_does_not_open_under_another_key_or_after_a_byte_changes() {
    let text = token::seal(&key(1, 7), &claims(), 1_000).unwrap();
    assert_eq!(
        token::open(&key(1, 8), body(&text), 1_000),
        Err(Refused::Invalid)
    );
    let mut changed = body(&text).to_owned();
    let last = changed.pop().unwrap();
    changed.push(if last == 'A' { 'B' } else { 'A' });
    assert_eq!(
        token::open(&key(1, 7), &changed, 1_000),
        Err(Refused::Invalid)
    );
    assert_eq!(
        token::open(&key(1, 7), "not base64!", 1_000),
        Err(Refused::Invalid)
    );
    assert_eq!(token::open(&key(1, 7), "", 1_000), Err(Refused::Invalid));
}

#[test]
fn a_token_expires_after_ten_minutes_and_one_issued_over_a_minute_ahead_is_refused() {
    let key = key(1, 7);
    let text = token::seal(&key, &claims(), 1_000).unwrap();
    let rest = body(&text);
    assert_eq!(
        token::open(&key, rest, 1_000 + LIFETIME),
        Err(Refused::Expired)
    );
    assert_eq!(token::open(&key, rest, 1_000 - 60), Ok(claims()));
    assert_eq!(token::open(&key, rest, 1_000 - 61), Err(Refused::Invalid));
}

#[test]
fn a_view_token_is_not_a_sign_in_token_of_the_same_key() {
    use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let key = key(1, 7);
    let aead = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key.bytes.expose()).unwrap());
    let mut sealed = br#"{"s":"x","v":1,"p":"bo@example.com","i":1000,"x":1600}"#.to_vec();
    aead.seal_in_place_append_tag(
        Nonce::assume_unique_for_key([3; 12]),
        Aad::from(b"flare-view-v1".as_slice()),
        &mut sealed,
    )
    .unwrap();
    let mut value = vec![3_u8; 12];
    value.extend_from_slice(&sealed);
    // Sealed under the sealing key itself rather than the key derived from
    // it, so the view key must not open it.
    assert_eq!(
        token::open(&key, &URL_SAFE_NO_PAD.encode(value), 1_000),
        Err(Refused::Invalid)
    );
}

#[test]
fn a_viewer_with_no_person_is_rebuilt_without_one() {
    let key = key(1, 7);
    let plain = Claims {
        viewer: Principal::new(PrincipalId::try_from("service").unwrap()),
        ..claims()
    };
    let text = token::seal(&key, &plain, 5).unwrap();
    assert_eq!(token::open(&key, body(&text), 5), Ok(plain));
}

#[tokio::test]
async fn a_host_mints_a_token_that_it_opens_and_refuses_one_that_names_an_unknown_key() {
    let host = host();
    let text = host.mint(&claims(), 1_000).await.unwrap();
    assert_eq!(host.open(&text, 1_000).await, Ok(claims()));
    assert_eq!(
        host.open(&text, 1_000 + LIFETIME).await,
        Err(ViewFailure::Expired)
    );
    let other = text.replacen("1.", "9.", 1);
    assert_eq!(host.open(&other, 1_000).await, Err(ViewFailure::Unknown));
    for garbage in ["", "x", "1", "0.AAAA", "1.", "-1.AAAA"] {
        assert_eq!(
            host.open(garbage, 1_000).await,
            Err(ViewFailure::Unknown),
            "{garbage}"
        );
    }
}
