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

#[test]
fn field_type_by_json_path() {
    let fields = json!({
        "object": {
            "name": "",
            "action": {
                "enable": false,
            },
        },
    });

    assert_eq!(
        find_field_by_json_tag(&fields, &["object"]),
        Some(&fields["object"])
    );
    assert_eq!(
        find_field_by_json_tag(&fields, &["object", "action"]),
        Some(&fields["object"]["action"])
    );
    assert_eq!(
        find_field_by_json_tag(&fields, &["object", "name"]),
        Some(&fields["object"]["name"])
    );
    assert_eq!(
        find_field_by_json_tag(&fields, &["object", "action", "enable"]),
        Some(&fields["object"]["action"]["enable"])
    );

    assert_eq!(find_field_by_json_tag(&fields, &["missing"]), None);
    assert_eq!(
        find_field_by_json_tag(&fields, &["object", "name", "nested"]),
        None
    );
    assert_eq!(find_field_by_json_tag(&fields, &[]), Some(&fields));
}
