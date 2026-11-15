use super::{Failure, GetReport, Got, parse};

#[test]
fn a_failure_report_parses_as_the_boards_failure_with_its_code() {
    let answer = parse::<GetReport>(
        br#"{"schema":"board/failure/v1","code":"board::cli::usage","message":"no --at","fields":{}}"#,
        "board/get-report/v2",
    );
    match answer {
        Err(Failure::Board(failure)) => assert_eq!(failure.code, "board::cli::usage"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_report_of_another_schema_is_a_child_failure() {
    let answer = parse::<GetReport>(
        br#"{"schema":"board/ls-report/v2","data":[]}"#,
        "board/get-report/v2",
    );
    assert!(matches!(answer, Err(Failure::Child(_))), "{answer:?}");
}

#[test]
fn a_get_report_answers_each_path_in_order() {
    let report: GetReport = parse(
        br#"{"schema":"board/get-report/v2","data":[{"location":{"path":"a.json","pointers":[],"source":"worktree"},"blob":"b1"},{"location":{"path":"c.json","pointers":[],"source":"worktree"},"blob":"b3"}],"failures":[{"index":1,"code":"board::path::not_found"},{"index":3,"code":"board::mount::stale"}]}"#,
        "board/get-report/v2",
    )
    .unwrap();
    let answers = report.per_path(4).unwrap();
    assert!(matches!(&answers[0], Got::Found(entry) if entry.blob == "b1"));
    assert_eq!(answers[1], Got::Missing);
    assert!(matches!(&answers[2], Got::Found(entry) if entry.blob == "b3"));
    assert_eq!(answers[3], Got::Failed("board::mount::stale".into()));
}
