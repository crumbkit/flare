use super::*;
use crate::{SearchError, SearchSpan};

#[test]
fn every_code_is_spelled_flare_area_kind_with_a_domain_noun_as_its_area() {
    let areas = [
        "shares",
        "grants",
        "versions",
        "recipients",
        "creators",
        "tokens",
        "activity",
        "requests",
        "teams",
        "comments",
        "cursors",
        "images",
        "search",
    ];
    for code in ApiCode::ALL {
        let parts: Vec<&str> = code.text().split("::").collect();
        assert_eq!(parts.len(), 3, "{}", code.text());
        assert_eq!(parts[0], "flare");
        assert!(areas.contains(&parts[1]), "{}", code.text());
        assert!(!parts[2].is_empty());
    }
}

#[test]
fn every_code_parses_back_from_its_text_and_an_unknown_text_does_not() {
    for code in ApiCode::ALL {
        assert_eq!(ApiCode::from_text(code.text()), Some(*code));
    }
    assert_eq!(ApiCode::from_text("flare::shares::nonsense"), None);
}

#[test]
fn a_problem_document_holds_its_code_status_and_recovery_as_crumbkit_writes_them() {
    let problem = ProblemDocument::new(ApiCode::SignInNeeded, "https://flare.example.org");
    let value = serde_json::to_value(&problem).unwrap();
    assert_eq!(value["status"], 401);
    assert_eq!(value["code"], "flare::tokens::sign_in_needed");
    assert_eq!(value["type"], "about:blank");
    assert_eq!(value["recovery"]["kind"], "run");
    assert_eq!(
        value["recovery"]["call"],
        "flare login https://flare.example.org"
    );
}

#[test]
fn the_not_a_creator_problem_names_the_operator_and_the_not_found_one_names_no_share() {
    let creator = ProblemDocument::new(ApiCode::NotACreator, "https://f.example");
    let Some(Recovery::Hint { text }) = &creator.recovery else {
        panic!("{creator:?}");
    };
    assert!(text.contains("operator"));
    let missing = ProblemDocument::new(ApiCode::ShareNotFound, "https://f.example");
    assert!(missing.share.is_none() && missing.detail.is_none());
}

#[test]
fn a_recipient_problem_shows_the_address_form() {
    let problem = ProblemDocument::new(ApiCode::NotAnAddress, "https://f.example");
    let Some(Recovery::Hint { text }) = &problem.recovery else {
        panic!("{problem:?}");
    };
    assert!(text.contains("ana@example.org"));
}

#[test]
fn the_team_codes_name_the_teams_area_and_the_everyone_refusal_names_grants() {
    assert_eq!(ApiCode::TeamNotFound.text(), "flare::teams::not_found");
    assert_eq!(ApiCode::TeamNotFound.status(), 404);
    assert_eq!(ApiCode::TeamNameTaken.text(), "flare::teams::name_taken");
    assert_eq!(
        ApiCode::TeamOwnerCannotLeave.text(),
        "flare::teams::owner_cannot_leave"
    );
    assert_eq!(ApiCode::TeamOwnerCannotLeave.status(), 409);
    assert_eq!(ApiCode::TeamNotAMember.text(), "flare::teams::not_a_member");
    assert_eq!(ApiCode::TeamNotAMember.status(), 409);
    assert_eq!(
        ApiCode::TeamsManagedElsewhere.text(),
        "flare::teams::managed_elsewhere"
    );
    assert_eq!(
        ApiCode::EveryoneUnbounded.text(),
        "flare::grants::everyone_unbounded"
    );
}

#[test]
fn the_comment_and_cursor_codes_carry_the_statuses_the_routes_answer() {
    for (code, text, status) in [
        (ApiCode::CommentNotFound, "flare::comments::not_found", 404),
        (
            ApiCode::NotCommentAuthor,
            "flare::comments::not_author",
            403,
        ),
        (ApiCode::ReplyDepth, "flare::comments::reply_depth", 400),
        (ApiCode::CommentInvalid, "flare::comments::invalid", 400),
        (ApiCode::TooManyComments, "flare::comments::too_many", 409),
        (
            ApiCode::CommentsUnavailable,
            "flare::comments::unavailable",
            503,
        ),
        (ApiCode::CursorNotFound, "flare::cursors::not_found", 404),
        (ApiCode::TooManyCursors, "flare::cursors::too_many", 409),
    ] {
        assert_eq!((code.text(), code.status()), (text, status));
    }
}

#[test]
fn a_bad_share_cursor_has_its_own_code_whose_hint_names_flare_shares() {
    let problem = ProblemDocument::new(ApiCode::SharesBadCursor, "https://f.example");
    assert_eq!(problem.code, "flare::shares::bad_cursor");
    assert_eq!(problem.status, 400);
    let Some(Recovery::Hint { text }) = &problem.recovery else {
        panic!("{problem:?}");
    };
    assert!(text.contains("flare shares"), "{text}");
    let activity = ProblemDocument::new(ApiCode::BadCursor, "https://f.example");
    assert_eq!(activity.code, "flare::activity::bad_cursor");
}

#[test]
fn a_creators_rule_that_cannot_answer_is_a_503_in_the_creators_area_that_asks_for_a_retry() {
    assert_eq!(
        ApiCode::CreatorsUnavailable.text(),
        "flare::creators::unavailable"
    );
    assert_eq!(ApiCode::CreatorsUnavailable.status(), 503);
    let problem = ProblemDocument::new(ApiCode::CreatorsUnavailable, "https://f.example");
    assert!(matches!(problem.recovery, Some(Recovery::Retry)));
}

#[test]
fn an_oversized_image_answers_413_every_other_image_refusal_422_and_the_format_refusal_advises_png_over_svg()
 {
    let image_codes: Vec<ApiCode> = ApiCode::ALL
        .iter()
        .copied()
        .filter(|code| code.text().starts_with("flare::images::"))
        .collect();
    let kinds: Vec<&str> = image_codes
        .iter()
        .map(|code| code.text().trim_start_matches("flare::images::"))
        .collect();
    assert_eq!(
        kinds,
        [
            "too_large",
            "too_many",
            "bad_path",
            "duplicate",
            "unused",
            "format_refused",
            "unreadable",
            "not_an_image",
            "too_many_pixels",
        ]
    );
    for code in image_codes {
        let expected = if code == ApiCode::ImageTooLarge {
            413
        } else {
            422
        };
        assert_eq!(code.status(), expected, "{}", code.text());
    }
    let Recovery::Hint { text } = ApiCode::ImageFormatRefused.recovery("https://f.example") else {
        panic!("the format refusal gives a hint");
    };
    assert!(text.contains("PNG") && text.contains("SVG"), "{text}");
}

#[test]
fn an_invalid_search_answers_422_and_lists_each_error_with_its_code_span_and_fix() {
    let error = SearchError {
        code: "flare::search::unknown_field".to_owned(),
        span: SearchSpan { start: 0, end: 4 },
        message: "There is no filter called form; did you mean from?".to_owned(),
    };
    let problem = ProblemDocument::new(ApiCode::SearchInvalid, "https://f.example")
        .with_errors(vec![error.clone()]);
    let value = serde_json::to_value(&problem).unwrap();
    assert_eq!(value["status"], 422);
    assert_eq!(value["code"], "flare::search::invalid");
    assert_eq!(value["errors"][0]["code"], "flare::search::unknown_field");
    assert_eq!(value["errors"][0]["span"]["end"], 4);
    let back: ProblemDocument = serde_json::from_value(value).unwrap();
    assert_eq!(back.errors, vec![error]);
    let person = ProblemDocument::new(ApiCode::UnknownPerson, "https://f.example");
    assert_eq!(person.status, 422);
    assert_eq!(person.code, "flare::search::unknown_person");
}

#[test]
fn a_problem_without_search_errors_has_no_errors_field() {
    let problem = ProblemDocument::new(ApiCode::ShareNotFound, "https://f.example");
    let value = serde_json::to_value(&problem).unwrap();
    assert!(value.get("errors").is_none(), "{value}");
    let read: ProblemDocument = serde_json::from_str(
        r#"{"type":"about:blank","title":"t","status":404,"code":"flare::shares::not_found"}"#,
    )
    .unwrap();
    assert!(read.errors.is_empty());
}

#[test]
fn a_username_deployment_has_its_own_codes_for_a_missing_username_and_an_address_recipient() {
    let no_username = ApiCode::from_text("flare::shares::no_username").expect("a code");
    assert_eq!(no_username.status(), 403);
    let not_a_username = ApiCode::from_text("flare::recipients::not_a_username").expect("a code");
    assert_eq!(not_a_username.status(), 400);
    assert!(not_a_username.title().contains("username"));
    // The email kind keeps its codes.
    assert_eq!(
        ApiCode::from_text("flare::shares::no_email").map(ApiCode::status),
        Some(403)
    );
}

#[test]
fn the_everyone_unbounded_refusal_names_the_setting_that_allows_it() {
    let Recovery::Hint { text } = ApiCode::EveryoneUnbounded.recovery("https://flare.example.org")
    else {
        panic!("a hint");
    };
    assert!(text.contains("hosted.everyone_signed_in"), "{text}");
}
