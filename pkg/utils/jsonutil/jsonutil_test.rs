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

#[test]
fn merge_json_object_fields_and_flags() {
    let mut old = json!({"name":"father","nested":{"known":1,"keep":true}});
    assert_eq!(
        super::merge_json_object(
            &mut old,
            br#"{"nested":{"known":2,"unknown":3},"unknown":0}"#
        )
        .unwrap(),
        (true, true)
    );
    assert_eq!(
        old,
        json!({"name":"father","nested":{"known":2,"keep":true}})
    );
    assert_eq!(
        super::merge_json_object(&mut old, br#"{"nested":{"known":2}}"#).unwrap(),
        (false, true)
    );
    assert_eq!(
        super::merge_json_object(&mut old, br#"{"unknown":0}"#).unwrap(),
        (false, false)
    );
    assert_eq!(
        super::merge_json_object(&mut old, b"{}").unwrap(),
        (false, false)
    );
}

#[test]
fn merge_json_object_invalid_inputs_are_unchanged() {
    let mut old = json!({"known":1});
    for input in [
        b"[]".as_slice(),
        b"1",
        b"true",
        b"\"text\"",
        b"{",
        b"{} trailing",
    ] {
        assert!(super::merge_json_object(&mut old, input).is_err());
        assert_eq!(old, json!({"known":1}));
    }
    assert_eq!(
        super::merge_json_object(&mut old, b"null").unwrap(),
        (false, false)
    );
    let mut scalar = json!(1);
    assert!(super::merge_json_object(&mut scalar, b"{}").is_err());
    assert_eq!(scalar, json!(1));
}

#[test]
fn merge_json_object_serialization_failure() {
    struct Fails;
    impl serde::Serialize for Fails {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("cannot serialize field"))
        }
    }
    impl<'de> serde::Deserialize<'de> for Fails {
        fn deserialize<D: serde::Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
            Err(serde::de::Error::custom("cannot deserialize field"))
        }
    }
    let error = super::merge_json_object(&mut Fails, b"{}").unwrap_err();
    assert!(error.to_string().contains("cannot serialize field"));
}
