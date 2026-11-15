use super::{BINDING_TYPE, EDIT_TYPE, PAGE_TYPE, schema_line};

#[test]
fn each_stored_type_has_a_schema_line_that_parses() {
    for record_type in [PAGE_TYPE, BINDING_TYPE, EDIT_TYPE] {
        let line = schema_line(record_type).unwrap();
        assert_eq!(line["path"], format!("/.board/types/{record_type}.schema"));
        assert_eq!(line["record"]["properties"]["$type"]["const"], record_type);
    }
}
