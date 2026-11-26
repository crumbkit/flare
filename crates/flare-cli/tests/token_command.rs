//! The share commands take their token from the deployment's token command
//! when the config's `client` section names one: the token reaches the
//! server as the bearer, nothing is saved, and each way the command fails
//! has its own code. Each refusal has a twin that shows the admitted case.
#![cfg(test)]

#[path = "support/api.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fake API is test support"
)]
pub mod api;
#[path = "../../flare-auth-loopback/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is flare-auth-loopback's test support, and these tests use part of it"
)]
pub mod loopback_scratch;
#[path = "support/run.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the run helper is test support, and each test file uses part of it"
)]
pub mod run;

use api::{FakeApi, Fakes as _, problem};
use run::Home;
use serde_json::{Value, json};
use std::path::Path;
use std::time::{Duration, Instant};

/// The token that the helper prints.
const PROXY_TOKEN: &str = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJhZGEifQ.c2lnbmF0dXJl";

/// A home whose config names `api` and a helper with `body`, and the
/// helper's path. The helper's deadline is `deadline` seconds when given.
fn with_helper(api: &FakeApi, body: &str, deadline: Option<u64>) -> (Home, String) {
    let home = Home::new();
    let helper = home.script("sso-token", body);
    let helper = helper.display().to_string();
    let mut client = json!({
        "server": api.origin,
        "token_command": [helper, "--audience", "flare"],
    });
    if let Some(seconds) = deadline {
        client["token_deadline_seconds"] = json!(seconds);
    }
    home.write_config(&json!({ "client": client }).to_string());
    (home, helper)
}

fn shares_answer(api: &FakeApi) {
    api.route(
        "GET",
        "/api/shares",
        200,
        &json!({"items": [], "more": false}),
    );
}

/// Every file under `dir`, read as bytes, that holds `needle`.
fn files_holding(dir: &Path, needle: &str) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_holding(&path, needle));
        } else if std::fs::read(&path)
            .is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(needle))
        {
            found.push(path.display().to_string());
        }
    }
    found
}

#[test]
fn the_token_commands_token_reaches_the_server_as_the_bearer_and_is_saved_nowhere() {
    let api = FakeApi::start();
    let seen = "printf '%s|%s|%s\\n' \"$FLARE_SERVER\" \"$1\" \"$2\" > \"$HOME/../seen.txt\"";
    let body = format!("{seen}\necho 'Signed in.' >&2\nprintf '%s\\n' '{PROXY_TOKEN}'\n");
    let (home, _helper) = with_helper(&api, &body, None);
    shares_answer(&api);
    let ran = home.sharing(&["shares", "--json"]);
    ran.assert_ok();
    let sent = api.requests_to("/api/shares");
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].headers.get("authorization").map(String::as_str),
        Some(format!("Bearer {PROXY_TOKEN}").as_str())
    );
    let seen = std::fs::read_to_string(home.path().join("seen.txt")).unwrap();
    assert_eq!(seen.trim_end(), format!("{}|--audience|flare", api.origin));
    assert!(ran.stderr.contains("Signed in."), "{}", ran.all());
    assert!(!ran.all().contains(PROXY_TOKEN), "{}", ran.all());
    assert!(
        home.keychain_calls().is_empty(),
        "{}",
        home.keychain_calls()
    );
    assert!(!home.state().join("flare").join("login.json").exists());
    let state_files = files_holding(&home.state(), PROXY_TOKEN);
    let home_files = files_holding(&home.path().join("home"), PROXY_TOKEN);
    assert!(
        state_files.is_empty() && home_files.is_empty(),
        "{state_files:?} {home_files:?}"
    );
}

#[test]
fn without_a_client_section_the_share_commands_need_a_login() {
    let api = FakeApi::start();
    let home = Home::new();
    shares_answer(&api);
    home.sharing(&["shares"])
        .assert_failed("flare::login::needed");
    assert!(api.requests_to("/api/shares").is_empty());
}

#[test]
fn a_helper_that_exits_3_fails_with_token_command_failed_naming_it_and_its_status() {
    let api = FakeApi::start();
    let (home, helper) = with_helper(&api, "exit 3\n", None);
    let ran = home.sharing(&["shares", "--json"]);
    ran.assert_failed("flare::login::token_command_failed");
    assert!(ran.all().contains(&helper), "{}", ran.all());
    assert!(ran.all().contains("status 3"), "{}", ran.all());
    assert!(api.requests().is_empty());
}

#[test]
fn a_helper_that_does_not_exist_fails_with_token_command_failed() {
    let api = FakeApi::start();
    let home = Home::new();
    home.write_config(
        &json!({ "client": {
            "server": api.origin,
            "token_command": [home.path().join("missing").display().to_string()],
        }})
        .to_string(),
    );
    home.sharing(&["shares"])
        .assert_failed("flare::login::token_command_failed");
}

#[test]
fn a_helper_past_its_deadline_is_killed_within_a_second_of_it() {
    let api = FakeApi::start();
    let (home, _helper) = with_helper(&api, "sleep 30\n", Some(1));
    let started = Instant::now();
    let ran = home.sharing(&["shares"]);
    let took = started.elapsed();
    ran.assert_failed("flare::login::token_command_timeout");
    assert!(took < Duration::from_secs(3), "took {took:?}");
    assert!(api.requests().is_empty());
}

#[test]
fn empty_output_too_much_output_and_two_words_fail_with_token_command_output_and_are_not_printed() {
    let api = FakeApi::start();
    let big = "head -c 20480 /dev/zero | tr '\\0' 'x'\necho\n".to_owned();
    for (body, shown) in [
        ("exit 0\n".to_owned(), None),
        (big, None),
        ("echo 'two words'\n".to_owned(), Some("two words")),
    ] {
        let (home, _helper) = with_helper(&api, &body, None);
        let ran = home.sharing(&["shares"]);
        ran.assert_failed("flare::login::token_command_output");
        if let Some(shown) = shown {
            assert!(!ran.all().contains(shown), "{}", ran.all());
        }
    }
    assert!(api.requests().is_empty());
}

#[test]
fn only_the_first_line_of_the_helpers_output_is_the_token() {
    let api = FakeApi::start();
    let body = format!("printf '%s\\n%s\\n' '{PROXY_TOKEN}' 'a second line'\n");
    let (home, _helper) = with_helper(&api, &body, None);
    shares_answer(&api);
    home.sharing(&["shares"]).assert_ok();
    let sent = api.requests_to("/api/shares");
    assert_eq!(
        sent[0].headers.get("authorization").map(String::as_str),
        Some(format!("Bearer {PROXY_TOKEN}").as_str())
    );
}

#[test]
fn a_refused_token_fails_with_sign_in_needed_whose_next_step_names_the_helper() {
    let api = FakeApi::start();
    let body = format!("printf '%s\\n' '{PROXY_TOKEN}'\n");
    let (home, helper) = with_helper(&api, &body, None);
    api.route(
        "GET",
        "/api/shares",
        401,
        &problem("flare::tokens::sign_in_needed", 401, &api.origin),
    );
    let ran = home.sharing(&["shares", "--json"]);
    ran.assert_failed("flare::tokens::sign_in_needed");
    let printed: Value = serde_json::from_str(ran.stdout.trim()).unwrap_or(Value::Null);
    let recovery = if printed.is_null() {
        ran.all()
    } else {
        printed.to_string()
    };
    assert!(recovery.contains(&helper), "{recovery}");
    assert!(!recovery.contains("flare login"), "{recovery}");
}

#[test]
fn logout_with_a_token_command_revokes_nothing_and_says_so() {
    let api = FakeApi::start();
    let body = format!("printf '%s\\n' '{PROXY_TOKEN}'\n");
    let (home, _helper) = with_helper(&api, &body, None);
    let ran = home.sharing(&["logout", "--yes", "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(ran.stdout.trim()).unwrap();
    assert_eq!(printed["data"]["revoked"], json!(false), "{printed}");
    assert!(api.requests().is_empty());
}
