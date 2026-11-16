use super::{cross_site, sign_in_host};
use crate::problem::Code;
use http::HeaderMap;
use http::header::HOST;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const PORT: u16 = 8740;

fn host(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(HOST, value.parse().unwrap());
    headers
}

fn fetch(dest: &str, site: &str, mode: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("sec-fetch-dest", dest.parse().unwrap());
    headers.insert("sec-fetch-site", site.parse().unwrap());
    headers.insert("sec-fetch-mode", mode.parse().unwrap());
    headers
}

#[test]
fn the_sign_in_host_refuses_a_peer_that_is_not_a_loopback_address() {
    let headers = host("localhost:8740");
    for peer in [
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        "::ffff:192.0.2.1".parse().unwrap(),
    ] {
        assert_eq!(
            sign_in_host(peer, &headers, PORT),
            Err(Code::Peer),
            "{peer}"
        );
    }
}

#[test]
fn the_sign_in_host_admits_each_loopback_peer_with_its_own_host() {
    let headers = host("localhost:8740");
    for peer in [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        "::ffff:127.0.0.1".parse().unwrap(),
    ] {
        assert_eq!(sign_in_host(peer, &headers, PORT), Ok(()), "{peer}");
    }
}

#[test]
fn the_sign_in_host_refuses_another_host_or_a_missing_one() {
    let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
    for value in ["localhost:8741", "127.0.0.1:8740", "LOCALHOST:8740"] {
        assert_eq!(
            sign_in_host(peer, &host(value), PORT),
            Err(Code::Host),
            "{value}"
        );
    }
    assert_eq!(sign_in_host(peer, &HeaderMap::new(), PORT), Err(Code::Host));
}

#[test]
fn a_credentialed_request_for_a_frame_is_refused_and_one_without_a_credential_is_not() {
    for dest in ["iframe", "frame", "IFRAME"] {
        let headers = fetch(dest, "same-origin", "navigate");
        assert_eq!(cross_site(&headers, true), Err(Code::Framed), "{dest}");
        assert_eq!(cross_site(&headers, false), Ok(()), "{dest}");
    }
}

#[test]
fn a_credentialed_top_level_navigation_is_admitted() {
    assert_eq!(
        cross_site(&fetch("document", "cross-site", "navigate"), true),
        Ok(())
    );
    assert_eq!(
        cross_site(&fetch("document", "same-origin", "navigate"), true),
        Ok(())
    );
    assert_eq!(
        cross_site(&fetch("image", "cross-site", "no-cors"), true),
        Err(Code::CrossSite)
    );
}
