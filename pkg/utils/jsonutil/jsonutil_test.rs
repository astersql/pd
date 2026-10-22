// Copyright 2026 AsterSQL.

use super::add_key_value;
use serde_json::json;

#[test]
fn add_key_value_flags() {
    let mut old =
        json!({"name": "father", "sub1": {"sub-name": "", "keep": true}, "sub2": {"sub-name": ""}});
    assert_eq!(
        add_key_value(&mut old, "sub1", json!({"sub-name": "son1"})),
        (true, true)
    );
    assert_eq!(old["sub1"], json!({"sub-name": "son1", "keep": true}));
    assert_eq!(
        add_key_value(&mut old, "sub2", json!({"sub-name": "son2"})),
        (true, true)
    );
    let snapshot = old.clone();
    assert_eq!(
        add_key_value(&mut old, "sub3", json!({"sub-name": "son2"})),
        (false, false)
    );
    assert_eq!(old, snapshot);
    assert_eq!(
        add_key_value(&mut old, "sub2", json!({"sub-name": "son2"})),
        (false, true)
    );
}

#[test]
fn add_key_value_existing_null_and_nested_unknown() {
    let mut old = json!({"optional": null, "nested": {"known": 1}});
    assert_eq!(
        add_key_value(&mut old, "optional", json!(null)),
        (false, true)
    );
    assert_eq!(
        add_key_value(&mut old, "optional", json!("value")),
        (true, true)
    );
    assert_eq!(
        add_key_value(&mut old, "nested", json!({"unknown": 2})),
        (false, true)
    );
    assert_eq!(old["nested"], json!({"known": 1}));
}
