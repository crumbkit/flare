//! `flare skills install`, `check` and `print`: an agent gets flare's skill
//! from the binary it runs, so the skill always names the commands that
//! binary has. Each test runs the built binary in a scratch home with an
//! empty `.agents/skills` directory, so no test touches the real home
//! directory.
#![cfg(test)]

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

use run::{Home, Ran};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The skill that the binary embeds.
const SKILL: &str = include_str!("../skill/SKILL.md");

/// A scratch home whose `.agents/skills` directory exists and is empty.
fn home() -> Home {
    let home = Home::new();
    std::fs::create_dir_all(skills_dir(&home)).unwrap();
    home
}

/// The home's `.agents/skills` directory.
fn skills_dir(home: &Home) -> PathBuf {
    home.path().join("home").join(".agents").join("skills")
}

/// The installed copy of the skill in the home's `.agents/skills`.
fn installed(home: &Home) -> PathBuf {
    skills_dir(home).join("flare").join("SKILL.md")
}

/// Parses a run's standard output as JSON.
fn json(ran: &Ran) -> Value {
    serde_json::from_str(&ran.stdout).unwrap_or_else(|error| panic!("{error}: {}", ran.all()))
}

/// The values of `field` in each row of the `--all --json` listing that
/// `args` prints.
fn listed(args: &[&str], field: &str) -> BTreeSet<String> {
    let home = Home::new();
    let mut all = args.to_vec();
    all.extend(["--all", "--json"]);
    let ran = home.run(&all);
    ran.assert_ok();
    json(&ran)["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row[field].as_str().unwrap().to_owned())
        .collect()
}

/// Each span between backticks in the skill's text. It pairs backticks by
/// position, so it reads inline spans only, and
/// `the_skill_holds_only_inline_code_spans` fails when the skill gains a
/// fenced block or an unpaired backtick that would shift the pairs.
fn backticked() -> Vec<&'static str> {
    SKILL.split('`').skip(1).step_by(2).collect()
}

#[test]
fn the_skill_holds_only_inline_code_spans() {
    assert!(
        !SKILL.contains("```") && !SKILL.contains("~~~"),
        "the skill holds a fenced code block, which the command and document tests cannot read"
    );
    assert_eq!(
        SKILL.matches('`').count() % 2,
        0,
        "the skill holds an unpaired backtick"
    );
}

#[test]
fn skills_install_writes_the_skill_with_metadata_naming_flare_and_a_second_install_changes_nothing()
{
    let home = home();

    let first = home.run(&["skills", "install", "--json"]);

    first.assert_ok();
    assert_eq!(
        json(&first)["data"][0]["state"],
        "written",
        "{}",
        first.all()
    );
    let text = std::fs::read_to_string(installed(&home)).unwrap();
    assert!(text.starts_with("---\nname: flare\n"), "{text}");
    assert!(
        text.contains("metadata:\n  tool: \"flare\"\n  version: \""),
        "{text}"
    );
    let second = home.run(&["skills", "install", "--json"]);
    second.assert_ok();
    assert_eq!(
        json(&second)["data"][0]["state"],
        "unchanged",
        "{}",
        second.all()
    );
}

#[test]
fn skills_check_exits_with_1_and_names_the_directory_when_the_copy_is_missing() {
    let home = home();

    let ran = home.run(&["skills", "check"]);

    assert_eq!(ran.code, Some(1), "{}", ran.all());
    assert!(ran.stdout.contains("missing"), "{}", ran.all());
    assert!(ran.stdout.contains(".agents/skills"), "{}", ran.all());
}

#[test]
fn skills_check_exits_with_0_and_prints_nothing_after_an_install() {
    let home = home();
    home.run(&["skills", "install"]).assert_ok();

    let ran = home.run(&["skills", "check"]);

    ran.assert_ok();
    assert_eq!(ran.stdout, "", "{}", ran.all());
}

#[test]
fn skills_check_reports_differs_and_exits_with_1_after_the_copy_is_edited() {
    let home = home();
    home.run(&["skills", "install"]).assert_ok();
    let edited = std::fs::read_to_string(installed(&home)).unwrap() + "An edit.\n";
    run::write(&installed(&home), &edited);

    let ran = home.run(&["skills", "check", "--json"]);

    assert_eq!(ran.code, Some(1), "{}", ran.all());
    assert_eq!(json(&ran)["data"][0]["state"], "differs", "{}", ran.all());
}

#[test]
fn skills_install_skips_a_skill_file_that_flare_did_not_write_and_leaves_it_as_it_was() {
    let home = home();
    std::fs::create_dir_all(installed(&home).parent().unwrap()).unwrap();
    run::write(&installed(&home), "someone else's skill\n");

    let ran = home.run(&["skills", "install", "--json"]);

    ran.assert_ok();
    assert_eq!(json(&ran)["data"][0]["state"], "skipped", "{}", ran.all());
    assert_eq!(
        std::fs::read_to_string(installed(&home)).unwrap(),
        "someone else's skill\n"
    );
}

#[test]
fn skills_install_with_force_replaces_a_skill_file_that_flare_did_not_write() {
    let home = home();
    std::fs::create_dir_all(installed(&home).parent().unwrap()).unwrap();
    run::write(&installed(&home), "someone else's skill\n");

    let ran = home.run(&["skills", "install", "--force", "--json"]);

    ran.assert_ok();
    assert_eq!(json(&ran)["data"][0]["state"], "written", "{}", ran.all());
    let text = std::fs::read_to_string(installed(&home)).unwrap();
    assert!(text.starts_with("---\nname: flare\n"), "{text}");
}

#[test]
fn skills_print_prints_the_skill_file_in_the_repository() {
    let home = home();

    let ran = home.run(&["skills", "print"]);

    ran.assert_ok();
    assert_eq!(ran.stdout, SKILL);
}

#[test]
fn the_skill_s_frontmatter_name_is_the_folder_that_install_writes() {
    let name = SKILL
        .strip_prefix("---\n")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("name: "))
        .unwrap();

    assert_eq!(name, "flare");
}

/// The skill names each command as a backticked span that starts with
/// `flare `, such as `flare share notes.md --with <address>`. The command
/// path is the leading words of lowercase letters and dashes; an argument
/// such as a file name, a `<placeholder>`, a document id or a flag ends it.
#[test]
fn each_command_the_skill_names_is_a_command_the_binary_lists() {
    let commands = listed(&["commands", "list"], "path");
    let mut named = 0;

    for span in backticked() {
        let Some(rest) = span.strip_prefix("flare ") else {
            continue;
        };
        let words: Vec<&str> = rest
            .split(' ')
            .take_while(|word| {
                word.starts_with(|c: char| c.is_ascii_lowercase())
                    && word.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            })
            .collect();
        let path = std::iter::once("flare")
            .chain(words)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            commands.contains(&path),
            "the skill names `{span}`, and flare has no command `{path}`"
        );
        named += 1;
    }

    assert!(named > 10, "the skill names only {named} commands");
}

#[test]
fn each_document_id_the_skill_names_is_a_document_the_binary_lists() {
    let documents = listed(&["schemas", "list"], "id");

    for word in backticked().iter().flat_map(|span| span.split(' ')) {
        if word.starts_with("flare/") && word.ends_with("/v1") {
            assert!(
                documents.contains(word),
                "the skill names {word}, and flare prints no such document"
            );
        }
    }
}
