use super::{Cookie, cookie, kept_state};
use crate::secret::{HOSTED_KEPT_COOKIE, KEPT_COOKIE};
use http::header::COOKIE;
use http::{HeaderMap, HeaderValue};

fn cookies(values: &[&[u8]]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for value in values {
        map.append(COOKIE, HeaderValue::from_bytes(value).unwrap());
    }
    map
}

fn kept(map: &HeaderMap, name: &str) -> Option<String> {
    kept_state(map, name).map(|kept| kept.expose().to_owned())
}

#[test]
fn the_kept_state_is_read_byte_by_byte_on_both_sites_and_two_are_none() {
    let one = cookies(&[b"a=\xff; __Host-flare_oidc_state=sealed; flare_oidc_state=local"]);
    assert_eq!(kept(&one, HOSTED_KEPT_COOKIE).as_deref(), Some("sealed"));
    assert_eq!(kept(&one, KEPT_COOKIE).as_deref(), Some("local"));
    let foreign = cookies(&[b"theme=caf\xc3\xa9", b"flare_oidc_state=local; z=\xff"]);
    assert_eq!(kept(&foreign, KEPT_COOKIE).as_deref(), Some("local"));
    let two = cookies(&[b"flare_oidc_state=a; flare_oidc_state=b"]);
    assert!(kept(&two, KEPT_COOKIE).is_none());
    let plain = cookies(&[b"flare_oidc_state=sealed"]);
    assert!(kept(&plain, HOSTED_KEPT_COOKIE).is_none());
}

#[test]
fn a_pair_is_one_cookie_only_when_its_value_is_a_token() {
    let token = cookies(&[b"bare; =x; name=tok-en_1  "]);
    assert!(matches!(cookie(&token, "name"), Cookie::One("tok-en_1")));
    for bad in [
        &b"name="[..],
        b"name=a\"b",
        b"name=a\\b",
        b"name=caf\xc3\xa9",
    ] {
        assert!(matches!(cookie(&cookies(&[bad]), "name"), Cookie::Bad));
    }
    assert!(matches!(
        cookie(&cookies(&[b"other=1"]), "name"),
        Cookie::Absent
    ));
}
