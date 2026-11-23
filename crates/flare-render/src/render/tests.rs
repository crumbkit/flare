//! Unit tests of rendering (threats 13 and 21; plan tests 6 to 9, 18 and
//! 19).

use super::*;
use crate::check::template;
use crate::check_page;
use crate::test_support::{
    Tok, html_page, markdown_page, record, slot, slot_name, start_tags, text_of, tokens,
};
use flare_core::{Page, RecordBody, RecordPath, RecordType};
use proptest::prelude::*;

const TASK_SAMPLE: &str = r#"{"title": "Replace the kitchen tap washer", "status": "open"}"#;

fn card(template: &str) -> Page {
    html_page(template, vec![slot("task", &["title"], Some(TASK_SAMPLE))])
}

fn render(template: &str, body: &str) -> Result<String, RenderError> {
    render_page(
        &card(template),
        &[(slot_name("task"), record("/tasks/a.md", '1', body))],
    )
    .map(PageDocument::into_string)
}

/// The text of the document's body.
fn body_text(document: &str) -> String {
    let start = document.find("<body>").unwrap();
    text_of(&document[start..])
}

#[test]
fn the_document_starts_with_the_stylesheet_in_a_style_element() {
    let document = render("<p>{{task.title}}</p>", TASK_SAMPLE).unwrap();
    assert!(document.starts_with("<!DOCTYPE html>"));
    let tags: Vec<String> = start_tags(&document)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(tags[..4], ["html", "head", "meta", "style"]);
    assert!(document.contains(crate::stylesheet()));
}

#[test]
fn a_filled_slot_shows_its_record_and_an_unfilled_slot_shows_its_sample() {
    let document = render("<h3>{{task.title}}</h3>", r#"{"title": "Ship it"}"#).unwrap();
    assert!(body_text(&document).contains("Ship it"));
    let document = render_page(&card("<h3>{{task.title}}</h3>"), &[]).unwrap();
    assert!(body_text(document.as_str()).contains("Replace the kitchen tap washer"));
}

#[test]
fn a_fill_of_the_wrong_type_or_an_undeclared_slot_is_a_misfit() {
    let other = Record::new(
        RecordPath::try_from("/notes/a").unwrap(),
        RecordType::try_from("notes/note/v1").unwrap(),
        crate::test_support::blob('1'),
        RecordBody::try_from("{}").unwrap(),
    );
    let result = render_page(&card("x"), &[(slot_name("task"), other)]);
    assert!(matches!(
        result,
        Err(RenderError::Misfit(Misfit::WrongType { .. }))
    ));
    let result = render_page(
        &card("x"),
        &[(slot_name("other"), record("/tasks/a.md", '1', "{}"))],
    );
    assert!(matches!(
        result,
        Err(RenderError::Misfit(Misfit::UndeclaredSlot { .. }))
    ));
}

#[test]
fn a_body_that_is_not_one_json_object_is_refused() {
    for body in ["[1]", "\"text\"", "title: x", ""] {
        assert!(
            matches!(
                render("{{task.title}}", body),
                Err(RenderError::NotAnObject { .. })
            ),
            "{body:?}"
        );
    }
}

#[test]
fn a_page_with_flaws_is_refused_before_rendering() {
    assert!(matches!(
        render("{{{task.title}}}", TASK_SAMPLE),
        Err(RenderError::Flaws(_))
    ));
}

#[test]
fn sections_iterate_lists_and_push_objects_and_inverted_sections_show_when_falsy() {
    let document = render(
        "<ul>{{#task.items}}<li>{{name}}</li>{{/task.items}}</ul>{{^task.none}}<p>none</p>{{/task.none}}{{#task.owner}}<b>{{person}}</b>{{/task.owner}}",
        r#"{"items": [{"name": "a"}, {"name": "b"}], "none": [], "owner": {"person": "ana"}}"#,
    )
    .unwrap();
    assert!(body_text(&document).contains("abnoneana"), "{document}");
}

#[test]
fn a_number_renders_as_its_json_text_and_a_boolean_as_true_or_false() {
    let document = render(
        "<p>{{task.weight}}|{{task.count}}|{{task.flag}}|{{task.nothing}}</p>",
        r#"{"weight": 1.50, "count": 7, "flag": true, "nothing": null}"#,
    )
    .unwrap();
    assert!(body_text(&document).contains("1.5|7|true|"), "{document}");
}

#[test]
fn a_variable_tag_that_names_a_list_or_an_object_is_not_text() {
    for template in ["<p>{{task.items}}</p>", "<p title=\"{{task.owner}}\">x</p>"] {
        let result = render(
            template,
            r#"{"items": ["<b>\" x=1"], "owner": {"k": "<i>"}}"#,
        );
        assert!(
            matches!(result, Err(RenderError::NotText { .. })),
            "{template}"
        );
    }
    let result = render(
        "{{#task.rows}}\n<p>{{cells}}</p>{{/task.rows}}",
        r#"{"rows": [{"cells": "fine"}, {"cells": ["<b>"]}]}"#,
    );
    let Err(RenderError::NotText { name, line, column }) = result else {
        panic!("expected NotText, got {result:?}");
    };
    assert_eq!((name.as_str(), line, column), ("cells", 2, 4));
    let result = render("{{#task}}{{.}}{{/task}}", r#"{"k": 1}"#);
    assert!(matches!(result, Err(RenderError::NotText { .. })));
}

#[test]
fn a_section_over_a_list_or_object_is_admitted() {
    let document = render(
        "{{#task.items}}<i>{{.}}</i>{{/task.items}}{{#task.owner}}{{k}}{{/task.owner}}",
        r#"{"items": ["x", "y"], "owner": {"k": "z"}}"#,
    )
    .unwrap();
    assert!(body_text(&document).contains("xyz"));
}

#[test]
fn nul_in_a_value_renders_as_the_replacement_character() {
    let document = render("<p>{{task.title}}</p>", r#"{"title": "a\u0000b"}"#).unwrap();
    assert!(document.contains("a\u{FFFD}b"));
    assert!(!document.contains('\0'));
}

#[test]
fn a_value_holding_mustache_tags_stays_text() {
    let document = render(
        "<p>{{task.title}}</p>",
        r#"{"title": "{{task.secret}} {{{x}}}"}"#,
    )
    .unwrap();
    assert!(body_text(&document).contains("{{task.secret}} {{{x}}}"));
}

#[test]
fn a_markdown_page_renders_as_prose_and_ignores_its_fills() {
    let document = render_page(&markdown_page("# Notes\n\n<script>x</script> {{x}}"), &[]).unwrap();
    let tags: Vec<String> = start_tags(document.as_str())
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(tags.contains(&"h1".to_owned()));
    assert!(!tags.contains(&"script".to_owned()));
    assert!(document.as_str().contains("class=\"prose-board\""));
}

#[test]
fn a_template_whose_worst_case_is_over_4_mib_is_refused_before_filling() {
    let items: Vec<String> = (0..64).map(|index| format!("\"{index}\"")).collect();
    let list = format!("[{}]", items.join(","));
    let body = format!(r#"{{"a": {list}, "b": {list}, "c": {list}, "d": {list}}}"#);
    let template = "{{#task.a}}{{#task.b}}{{#task.c}}{{#task.d}}<p>some text here</p>{{/task.d}}{{/task.c}}{{/task.b}}{{/task.a}}";
    assert!(matches!(
        render(template, &body),
        Err(RenderError::TooLarge {
            max: MAX_OUTPUT_BYTES
        })
    ));
}

#[test]
fn nested_sections_over_long_lists_with_empty_bodies_are_refused_quickly() {
    let items: Vec<String> = (0..1000).map(|index| index.to_string()).collect();
    let list = format!("[{}]", items.join(","));
    let body = format!(r#"{{"a": {list}}}"#);
    let open = "{{#task.a}}".repeat(8);
    let close = "{{/task.a}}".repeat(8);
    assert!(matches!(
        render(&format!("{open}{close}"), &body),
        Err(RenderError::TooLarge { .. } | RenderError::TooCostly { .. })
    ));
}

/// Renders `template` with the slots `a` and `b` filled with `a` and `b`.
fn render_two(template: &str, a: &str, b: &str) -> Result<String, RenderError> {
    render_page(
        &html_page(template, vec![slot("a", &[], None), slot("b", &[], None)]),
        &[
            (slot_name("a"), record("/tasks/a.md", '1', a)),
            (slot_name("b"), record("/tasks/b.md", '2', b)),
        ],
    )
    .map(PageDocument::into_string)
}

#[test]
fn a_section_that_copies_a_large_record_on_each_pass_of_a_long_loop_is_refused_quickly() {
    // mustache2 copies the value a section pushes, and the list a section
    // walks, each time it enters the section. Here each of the 500,000
    // passes over a.items would copy all of b, which is near 1 MiB.
    let a = format!(r#"{{"items": [{}0]}}"#, "0,".repeat(499_999));
    let b = format!(r#"{{"l": [{}"a"]}}"#, "\"a\",".repeat(249_999));
    assert!(a.len() < 1024 * 1024 && b.len() < 1024 * 1024);
    let template = "{{#a.items}}{{#b}}{{/b}}{{/a.items}}";
    assert!(
        check_page(&html_page(
            template,
            vec![slot("a", &[], None), slot("b", &[], None)]
        ))
        .is_ok()
    );
    assert!(matches!(
        render_two(template, &a, &b),
        Err(RenderError::TooCostly {
            max: MAX_COPIED_BYTES
        })
    ));
}

#[test]
fn a_tag_with_a_name_of_a_million_bytes_inside_a_long_loop_is_refused_by_the_check() {
    let a = format!(r#"{{"items": [{}0]}}"#, "0,".repeat(499_999));
    let template = format!(
        "{{{{#a.items}}}}{{{{{}}}}}{{{{/a.items}}}}",
        "x".repeat(1_000_000)
    );
    assert!(matches!(
        render_two(&template, &a, "{}"),
        Err(RenderError::Flaws(_))
    ));
}

#[test]
fn a_256_byte_name_looked_up_on_each_pass_of_a_long_loop_is_refused_by_the_cost_budget() {
    let a = format!(r#"{{"items": [{}0]}}"#, "0,".repeat(499_999));
    let template = format!(
        "{{{{#a.items}}}}{{{{{}}}}}{{{{/a.items}}}}",
        "x".repeat(256)
    );
    assert!(matches!(
        render_two(&template, &a, "{}"),
        Err(RenderError::TooCostly {
            max: MAX_COPIED_BYTES
        })
    ));
}

#[test]
fn a_list_section_counts_one_copy_of_the_list_and_one_of_each_element() {
    // The lookup of a.items on a stack of one counts 7 bytes. The list
    // ["ab", "cd"] counts 32 + (32 + 2) * 2 = 100 bytes, and each element 34
    // more.
    assert_eq!(
        walk::copied("{{#a.items}}{{/a.items}}", r#"{"items": ["ab", "cd"]}"#),
        175
    );
}

#[test]
fn a_loop_over_a_long_list_of_numbers_in_one_record_is_admitted() {
    let a = format!(r#"{{"items": [{}0]}}"#, "7,".repeat(99_999));
    let document = render_two("<p>{{#a.items}}{{.}} {{/a.items}}</p>", &a, "{}").unwrap();
    let body = &document[document.find("<body>").unwrap()..];
    assert_eq!(body.matches("7 ").count(), 99_999);
}

#[test]
fn a_nested_loop_over_tasks_and_their_tags_is_admitted() {
    let tasks: Vec<String> = (0..200)
        .map(|index| format!(r#"{{"title": "Task {index}", "tags": ["home", "garden", "tools", "weekend", "later"]}}"#))
        .collect();
    let a = format!(r#"{{"items": [{}]}}"#, tasks.join(","));
    let template = "<ul>{{#a.items}}<li>{{title}}{{#tags}} <b>{{.}}</b>{{/tags}}{{#a}}{{/a}}</li>{{/a.items}}</ul>";
    let document = render_two(template, &a, "{}").unwrap();
    assert_eq!(document.matches("<li>").count(), 200);
    assert_eq!(document.matches("<b>garden</b>").count(), 200);
}

#[test]
fn a_markdown_page_whose_html_is_over_4_mib_is_refused() {
    let text = "<>".repeat(512 * 1024);
    assert!(matches!(
        render_page(&markdown_page(&text), &[]),
        Err(RenderError::TooLarge { .. })
    ));
}

/// The start tags and attribute names of `html`.
fn shape(html: &str) -> Vec<(String, Vec<String>)> {
    tokens(html)
        .into_iter()
        .filter_map(|token| match token {
            Tok::Start { name, attributes } => {
                Some((name, attributes.into_iter().map(|(key, _)| key).collect()))
            }
            _ => None,
        })
        .collect()
}

const VALUE_TEMPLATE: &str = "<p title=\"{{task.v}}\">{{task.v}}</p><a href=\"/x\" class=\"c {{task.v}}\">{{task.v}}</a><svg><text aria-label=\"{{task.v}}\">{{task.v}}</text></svg>";

fn with_value(value: &str) -> String {
    render(
        VALUE_TEMPLATE,
        &serde_json::json!({ "v": value }).to_string(),
    )
    .unwrap()
}

/// The text a browser's tokenizer reads from a value: NUL becomes U+FFFD
/// when rendered, and the tokenizer turns CR and CRLF into LF.
fn as_read(value: &str) -> String {
    value
        .replace('\0', "\u{FFFD}")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// Templates built from pieces, so that many of them pass the check.
fn templates() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        8 => prop::sample::select(vec![
            "text ", "<p>", "</p>", "<p title=\"", "\">", "{{task.title}}", "{{task.items}}",
            "{{#task.items}}", "{{/task.items}}", "{{^task.items}}", "{{.}}", "{{name}}",
            "{{#task.owner}}", "{{/task.owner}}", "{{! c }}", "\n", "  ", "{{task.n}}", "&",
        ])
        .prop_map(str::to_owned),
        1 => "\\PC{0,4}",
    ];
    prop::collection::vec(piece, 0..20).prop_map(|pieces| pieces.concat())
}

fn json_value() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(serde_json::Value::from),
        "\\PC{0,8}".prop_map(serde_json::Value::String),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::Array),
            prop::collection::btree_map(
                prop::sample::select(vec!["title", "items", "name", "owner", "n"])
                    .prop_map(str::to_owned),
                inner,
                0..4,
            )
            .prop_map(|map| serde_json::Value::Object(map.into_iter().collect())),
        ]
    })
}

fn object() -> impl Strategy<Value = String> {
    prop::collection::btree_map(
        prop::sample::select(vec!["title", "items", "name", "owner", "n"]).prop_map(str::to_owned),
        json_value(),
        0..5,
    )
    .prop_map(|map| serde_json::Value::Object(map.into_iter().collect()).to_string())
}

proptest! {
    #[test]
    fn an_arbitrary_value_adds_no_element_or_attribute(value in "\\PC*", control in "[\\x00-\\x1f\"'<>&=`/]{0,8}") {
        let expected = shape(&with_value("plain"));
        prop_assert_eq!(shape(&with_value(&value)), expected.clone());
        prop_assert_eq!(shape(&with_value(&control)), expected);
    }

    #[test]
    fn any_string_renders_as_its_escaped_self(value in "\\PC*", control in "[\\x00-\\x1f{}\"'<>&]{0,8}") {
        for value in [value, control, "{{task.v}} {{{x}}}".to_owned()] {
            let document = render("<p>{{task.v}}</p><p title=\"{{task.v}}\">x</p>", &serde_json::json!({ "v": &value }).to_string()).unwrap();
            let body = &document[document.find("<body>").unwrap()..];
            let mut text = String::new();
            let mut inside = false;
            for token in tokens(body) {
                match token {
                    Tok::Start { name, attributes } if name == "p" && attributes.is_empty() => inside = true,
                    Tok::End { name } if name == "p" => break,
                    Tok::Text(part) if inside => text.push_str(&part),
                    _ => {}
                }
            }
            prop_assert_eq!(text, as_read(&value));
            let title = start_tags(body).into_iter().find_map(|(name, attributes)| {
                (name == "p").then_some(()).and(attributes.into_iter().find(|(key, _)| key == "title").map(|(_, value)| value))
            }).unwrap();
            prop_assert_eq!(title, as_read(&value));
        }
    }

    #[test]
    fn the_output_is_never_longer_than_the_worst_case(text in templates(), data in object()) {
        let page = card(&text);
        let Ok(Some(nodes)) = template(&page) else { return Ok(()); };
        let data = data::page_data(&page, &[(slot_name("task"), record("/tasks/a.md", '1', &data))]).unwrap();
        let Ok(bound) = walk::worst_case(text.as_str(), &nodes, &data) else { return Ok(()); };
        let output = fill(&text, data).unwrap();
        prop_assert!(output.len() <= bound, "{} > {}", output.len(), bound);
    }

    #[test]
    fn rendering_never_panics_for_an_admitted_template_and_any_data(text in templates(), data in object()) {
        let _ = render(&text, &data);
    }
}
