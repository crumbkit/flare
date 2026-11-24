use super::Failed;
use crumbkit::{Failure as _, Field};

/// The message and the fields of `failed`.
fn shown(failed: &Failed) -> (String, Vec<Field<'_>>) {
    (failed.to_string(), failed.fields())
}

#[test]
fn program_unavailable_prints_the_program_it_names_and_gives_it_as_a_field() {
    let named = Failed::ProgramUnavailable {
        program: Some("/opt/board".to_owned()),
        problem: "No such file or directory (os error 2)".to_owned(),
    };
    assert_eq!(
        shown(&named),
        (
            "the board program /opt/board: No such file or directory (os error 2)".to_owned(),
            vec![Field::new("program", "/opt/board")]
        )
    );
    let unnamed = Failed::ProgramUnavailable {
        program: None,
        problem: "board was not found on PATH".to_owned(),
    };
    assert_eq!(
        shown(&unnamed),
        (
            "the board program board was not found on PATH".to_owned(),
            vec![Field::new("program", None::<&str>)]
        )
    );
}

#[test]
fn state_unusable_prints_the_directory_it_names_and_gives_it_as_a_field() {
    let named = Failed::StateUnusable {
        path: Some("/state/flare/board/key".to_owned()),
        problem: "Not a directory (os error 20)".to_owned(),
    };
    assert_eq!(
        shown(&named),
        (
            "/state/flare/board/key: Not a directory (os error 20)".to_owned(),
            vec![Field::new("path", "/state/flare/board/key")]
        )
    );
    let unnamed = Failed::StateUnusable {
        path: None,
        problem: "the saved login could not be used: it is not JSON".to_owned(),
    };
    assert_eq!(
        shown(&unnamed),
        (
            "the saved login could not be used: it is not JSON".to_owned(),
            vec![Field::new("path", None::<&str>)]
        )
    );
}

#[test]
fn opener_failed_prints_the_program_it_names_and_gives_it_as_a_field() {
    let named = Failed::OpenerFailed {
        program: Some("/usr/bin/open".to_owned()),
        problem: "ran past its deadline of 10 seconds".to_owned(),
    };
    assert_eq!(
        shown(&named),
        (
            "the program that opens the browser failed: /usr/bin/open ran past its deadline of 10 seconds"
                .to_owned(),
            vec![Field::new("program", "/usr/bin/open")]
        )
    );
    let unnamed = Failed::OpenerFailed {
        program: None,
        problem: "the server's answer names no opener file".to_owned(),
    };
    assert_eq!(
        shown(&unnamed),
        (
            "the program that opens the browser failed: the server's answer names no opener file"
                .to_owned(),
            vec![Field::new("program", None::<&str>)]
        )
    );
}
