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
