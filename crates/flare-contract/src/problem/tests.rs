use super::*;

#[test]
fn every_code_is_spelled_flare_area_kind_with_a_domain_noun_as_its_area() {
    let areas = [
        "shares",
        "grants",
        "versions",
        "recipients",
        "tokens",
        "activity",
        "requests",
        "comments",
        "cursors",
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
