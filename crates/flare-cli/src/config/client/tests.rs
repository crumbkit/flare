//! The `client` section: the server and the token command that the share
//! commands use, and the deadline that bounds the command. Each refusal has
//! a twin that shows the admitted case.

use super::super::{Problem, parse};
use std::time::Duration;

fn client(extra: &str) -> String {
    format!(
        r#"{{"client":{{"server":"https://flare.example.com","token_command":["/usr/local/bin/sso-token","--audience","flare"]{extra}}}}}"#
    )
}

fn refusal(text: &str) -> String {
    match parse(text) {
        Err(Problem::Invalid(message)) => message,
        Ok(_) => panic!("accepted: {text}"),
    }
}

#[test]
fn a_client_section_names_the_server_and_the_command_with_a_20_second_deadline() {
    let config = parse(&client("")).unwrap();
    let client = config.client.unwrap();
    assert_eq!(client.server, "https://flare.example.com");
    assert_eq!(client.command.program, "/usr/local/bin/sso-token");
    assert_eq!(client.command.args, ["--audience", "flare"]);
    assert_eq!(client.command.deadline, Duration::from_secs(20));
}

#[test]
fn a_deadline_from_1_to_120_seconds_is_read_and_one_outside_it_refused() {
    for seconds in [1, 120] {
        let config = parse(&client(&format!(",\"token_deadline_seconds\":{seconds}"))).unwrap();
        assert_eq!(
            config.client.unwrap().command.deadline,
            Duration::from_secs(seconds)
        );
    }
    for seconds in [0, 121] {
        let message = refusal(&client(&format!(",\"token_deadline_seconds\":{seconds}")));
        assert!(
            message.contains("client.token_deadline_seconds"),
            "{message}"
        );
    }
}

#[test]
fn an_empty_command_or_program_is_refused() {
    for command in ["[]", "[\"\"]"] {
        let text = client("").replacen(
            r#"["/usr/local/bin/sso-token","--audience","flare"]"#,
            command,
            1,
        );
        assert!(refusal(&text).contains("client.token_command"));
    }
}

#[test]
fn a_server_that_is_not_an_https_origin_is_refused() {
    let text = client("").replacen("https://flare.example.com", "http://flare.example.com", 1);
    assert!(refusal(&text).contains("client.server"));
    let path = client("").replacen(
        "https://flare.example.com",
        "https://flare.example.com/x",
        1,
    );
    assert!(refusal(&path).contains("client.server"));
}

#[test]
fn a_config_without_a_client_section_has_no_client() {
    assert!(parse("{}").unwrap().client.is_none());
}
