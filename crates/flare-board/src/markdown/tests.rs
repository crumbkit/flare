use super::{MarkdownError, field_order, from_object, split};
use serde_json::{Map, Value, json};

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => panic!("not an object"),
    }
}

const TASK: &str = "---\n$type: tasks/task/v2\ntitle: \"T\"\nstatus: open\nparent: /tasks/p\n---\nline\n---\nafter\n";

#[test]
fn split_answers_the_prose_after_the_closing_line() {
    assert_eq!(
        split(TASK),
        Some((
            "$type: tasks/task/v2\ntitle: \"T\"\nstatus: open\nparent: /tasks/p\n",
            "line\n---\nafter\n"
        ))
    );
}

#[test]
fn split_ends_a_block_at_three_dots_too() {
    assert_eq!(split("---\na: 1\n...\nprose"), Some(("a: 1\n", "prose")));
}

#[test]
fn split_answers_none_for_a_file_without_front_matter() {
    assert_eq!(split("# Title\n"), None);
    assert_eq!(split("---\na: 1\n"), None);
}

#[test]
fn field_order_lists_the_top_level_keys_in_the_files_order() {
    let block = "$type: tasks/task/v2\ntitle: \"T\"\nafter:\n  - /tasks/b\n# a comment\n\"quoted key\": 1\nstatus: open\n";
    assert_eq!(
        field_order(block),
        ["$type", "title", "after", "quoted key", "status"]
    );
}

#[test]
fn a_file_keeps_the_field_order_puts_new_fields_last_and_drops_removed_ones() {
    let order: Vec<String> = ["$type", "title", "status", "parent"]
        .map(String::from)
        .into();
    let wanted = object(json!({
        "$body": "line\n---\nafter\n",
        "$type": "tasks/task/v2",
        "due": "2026-11-20",
        "status": "done",
        "title": "T",
        "contexts": ["errands"],
    }));
    assert_eq!(
        from_object(&wanted, &order),
        Ok("---\n$type: \"tasks/task/v2\"\ntitle: \"T\"\nstatus: \"done\"\ncontexts: [\"errands\"]\ndue: \"2026-11-20\"\n---\nline\n---\nafter\n".to_owned())
    );
}

#[test]
fn every_kind_of_value_is_written_as_one_json_line() {
    let wanted = object(json!({
        "$type": "tasks/task/v2",
        "title": "a \"quoted\": #tag\\n\nnext",
        "after": ["/tasks/b"],
        "ephemeral": true,
        "from_hour": 9,
        "pinned": 3,
        "owner": {"person": "ana"},
        "odd key": null,
        "$body": "",
    }));
    let file = from_object(&wanted, &[]).unwrap();
    assert_eq!(
        file,
        "---\n$type: \"tasks/task/v2\"\nafter: [\"/tasks/b\"]\nephemeral: true\nfrom_hour: 9\n\"odd key\": null\nowner: {\"person\":\"ana\"}\npinned: 3\ntitle: \"a \\\"quoted\\\": #tag\\\\n\\nnext\"\n---\n"
    );
}

#[test]
fn an_object_without_its_prose_or_its_type_is_refused() {
    assert_eq!(
        from_object(&object(json!({"$type": "tasks/task/v2"})), &[]),
        Err(MarkdownError::NoBody)
    );
    assert_eq!(
        from_object(&object(json!({"$type": "tasks/task/v2", "$body": 5})), &[]),
        Err(MarkdownError::NoBody)
    );
    assert_eq!(
        from_object(&object(json!({"title": "t", "$body": ""})), &[]),
        Err(MarkdownError::NoType)
    );
}
