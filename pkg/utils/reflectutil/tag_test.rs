// Copyright 2026 AsterSQL.

use serde_json::json;

use super::*;

#[test]
fn full_tag_by_child_tag() {
    let fields = json!({
        "object": {
            "name": "",
            "action": {
                "enable": false,
            },
        },
    });

    assert_eq!(
        find_json_full_tag_by_child_tag(&fields, "enable"),
        "object.action.enable"
    );
    assert_eq!(
        find_json_full_tag_by_child_tag(&fields, "action"),
        "object.action"
    );
    assert_eq!(find_json_full_tag_by_child_tag(&fields, "disable"), "");
}

#[test]
fn same_fields_by_json() {
    let fields = json!({
        "name": "",
        "action": {
            "enable": false,
        },
    });

    assert!(find_same_field_by_json(&fields, &json!({"name": "test2"})));
    assert!(!find_same_field_by_json(
        &fields,
        &json!({"enable": "test2"})
    ));
    assert!(!find_same_field_by_json(&fields, &json!({"missing": null})));
}
