use super::{Found, MAX_HITS, SearchError};
use crate::contract::block_on;
use crate::flares::{Creators, Flares, NamedRecipients, NewFlare, RecipientError, Recipients};
use crate::memory::{MemoryGrantStore, MemoryResponseStore, MemoryShareStore};
use crate::sharing::ShareError;
use crate::{
    DocumentText, Email, MAX_LISTED, Principal, PrincipalId, ShareContent, ShareId, ShareTitle,
    Timestamp,
};
use flare_query::{ErrorCode, Span, parse};

type Memory = Flares<MemoryShareStore, MemoryGrantStore, MemoryResponseStore>;

fn email(text: &str) -> Email {
    Email::try_from(text).unwrap()
}

fn person(address: &str) -> Principal {
    Principal::with_email(PrincipalId::try_from(address).unwrap(), email(address))
}

fn memory() -> Memory {
    Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        Creators::Everyone,
    )
}

fn id(number: u16) -> ShareId {
    let mut bytes = [0; 16];
    bytes[..2].copy_from_slice(&number.to_be_bytes());
    ShareId::from_random(bytes)
}

fn share(flares: &Memory, number: u16, owner: &str, title: &str, with: &[&str]) {
    let flare = NewFlare::new(
        id(number),
        ShareTitle::try_from(title).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        with.iter().map(|text| email(text)).collect(),
        Vec::new(),
    );
    let at = Timestamp::from_secs(u64::from(number) * 10);
    block_on(flares.share(&person(owner), flare, at)).unwrap();
}

/// A directory that knows ana by name, or that does not answer.
struct Directory {
    down: bool,
}

impl Recipients for Directory {
    async fn resolve(&self, identifier: &str) -> Result<Email, RecipientError> {
        if self.down {
            return Err(RecipientError::Unavailable);
        }
        NamedRecipients::new([("ana".to_owned(), email("ana@example.org"))])
            .resolve(identifier)
            .await
    }
}

const UP: Directory = Directory { down: false };

fn run(flares: &Memory, who: &Principal, text: &str, limit: usize) -> Result<Found, SearchError> {
    block_on(flares.search(who, &parse(text).unwrap(), &UP, limit))
}

fn invalid(result: Result<Found, SearchError>) -> Vec<(ErrorCode, Span)> {
    match result {
        Err(SearchError::Invalid(errors)) => errors
            .into_iter()
            .map(|error| (error.code, error.span))
            .collect(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_word_matches_a_title_in_any_case_and_inside_a_longer_word() {
    let flares = memory();
    share(&flares, 1, "mei@example.org", "Quarterly BUDGETS", &[]);
    share(&flares, 2, "mei@example.org", "Team notes", &[]);
    let found = run(&flares, &person("mei@example.org"), "budget", 50).unwrap();
    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].share.id(), &id(1));
    assert!(!found.partial);
}

#[test]
fn a_from_name_the_directory_does_not_know_is_an_unknown_person_at_its_span() {
    let flares = memory();
    share(&flares, 1, "ana@example.org", "Plan", &["mei@example.org"]);
    let mei = person("mei@example.org");
    let errors = invalid(run(&flares, &mei, "plan from:me,nobody", 50));
    assert_eq!(errors, [(ErrorCode::UnknownPerson, Span::new(13, 19))]);
    let twin = run(&flares, &mei, "plan from:me,ana", 50).unwrap();
    assert_eq!(twin.hits.len(), 1);
}

#[test]
fn unknown_types_and_people_are_all_listed_in_the_order_of_the_text() {
    let flares = memory();
    let mei = person("mei@example.org");
    let errors = invalid(run(&flares, &mei, "from:x type:pdf from:me", 50));
    assert_eq!(
        errors,
        [
            (ErrorCode::UnknownPerson, Span::new(5, 6)),
            (ErrorCode::UnknownType, Span::new(12, 15)),
        ]
    );
}

#[test]
fn a_directory_that_does_not_answer_stops_the_search_and_me_needs_no_directory() {
    let flares = memory();
    share(&flares, 1, "mei@example.org", "Plan", &[]);
    let mei = person("mei@example.org");
    let down = Directory { down: true };
    let query = parse("from:ana").unwrap();
    let result = block_on(flares.search(&mei, &query, &down, 50));
    assert_eq!(result.unwrap_err(), SearchError::RecipientsUnavailable);
    let me = parse("from:me").unwrap();
    let found = block_on(flares.search(&mei, &me, &down, 50)).unwrap();
    assert_eq!(found.hits.len(), 1);
}

#[test]
fn a_caller_without_a_verified_address_is_refused() {
    let flares = memory();
    let local = Principal::new(PrincipalId::try_from("local:mei").unwrap());
    let result = run(&flares, &local, "budget", 50);
    assert_eq!(
        result.unwrap_err(),
        SearchError::Share(ShareError::NoVerifiedEmail)
    );
}

#[test]
fn type_html_matches_no_share_and_minus_type_html_matches_every_one() {
    let flares = memory();
    share(&flares, 1, "mei@example.org", "Plan", &[]);
    let mei = person("mei@example.org");
    assert!(run(&flares, &mei, "type:html", 50).unwrap().hits.is_empty());
    assert_eq!(run(&flares, &mei, "-type:html", 50).unwrap().hits.len(), 1);
    assert_eq!(
        run(&flares, &mei, "type:Markdown", 50).unwrap().hits.len(),
        1
    );
}

#[test]
fn a_search_answers_at_most_its_limit_newest_first_and_never_more_than_the_most() {
    let flares = memory();
    for number in 1..=5 {
        share(&flares, number, "mei@example.org", "Plan", &[]);
    }
    let mei = person("mei@example.org");
    let found = run(&flares, &mei, "", 2).unwrap();
    let ids: Vec<&ShareId> = found.hits.iter().map(|hit| hit.share.id()).collect();
    assert_eq!(ids, [&id(5), &id(4)]);
    assert_eq!(run(&flares, &mei, "", 0).unwrap().hits.len(), 1);
    assert_eq!(run(&flares, &mei, "", MAX_HITS + 9).unwrap().hits.len(), 5);
}

#[test]
fn a_search_is_partial_when_the_shares_granted_to_the_caller_pass_the_list_limit() {
    let flares = memory();
    let count = u16::try_from(MAX_LISTED).unwrap() + 1;
    for number in 1..=count {
        let owner = if number % 2 == 0 {
            "ana@example.org"
        } else {
            "jo@example.org"
        };
        share(&flares, number, owner, "Plan", &["mei@example.org"]);
    }
    let found = run(&flares, &person("mei@example.org"), "plan", MAX_HITS).unwrap();
    assert!(found.partial);
    assert_eq!(found.hits.len(), MAX_HITS);
    let twin = run(&flares, &person("ana@example.org"), "plan", MAX_HITS).unwrap();
    assert!(!twin.partial);
}
