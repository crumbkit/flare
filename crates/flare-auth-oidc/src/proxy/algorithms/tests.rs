//! Unit tests of the proxy's algorithms: each refusal beside what it
//! admits.

use super::*;

#[test]
fn rs256_and_es256_are_the_only_algorithms() {
    assert_eq!(ProxyAlgorithm::try_from("RS256"), Ok(ProxyAlgorithm::Rs256));
    assert_eq!(ProxyAlgorithm::try_from("ES256"), Ok(ProxyAlgorithm::Es256));
    for text in ["none", "HS256", "RS512", "PS256", "EdDSA", "rs256"] {
        assert_eq!(
            ProxyAlgorithm::try_from(text),
            Err(ProxyConfigError::Algorithm),
            "{text}"
        );
    }
}

#[test]
fn an_empty_algorithm_list_is_refused_and_a_repeated_one_kept_once() {
    assert_eq!(ProxyAlgorithms::new(&[]), Err(ProxyConfigError::Algorithm));
    let both = ProxyAlgorithms::new(&[ProxyAlgorithm::Es256, ProxyAlgorithm::Es256]).unwrap();
    assert!(both.lists("ES256"));
    assert!(!both.lists("RS256"));
}
