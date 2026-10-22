// Copyright 2026 AsterSQL.

use super::deep_clone_json;
use std::collections::BTreeMap;

#[test]
fn deep_clone_contract() {
    let source = BTreeMap::from([(String::from("attributes"), vec![1_u64, 2])]);
    let mut cloned = deep_clone_json(&source).unwrap();
    assert_eq!(source, cloned);
    cloned.get_mut("attributes").unwrap().push(3);
    assert_eq!(source["attributes"], [1, 2]);
    let absent: Option<Vec<u64>> = None;
    assert_eq!(deep_clone_json(&absent).unwrap(), None);
    let present = Some(vec![7_u64]);
    let mut cloned = deep_clone_json(&present).unwrap();
    cloned.as_mut().unwrap().push(8);
    assert_eq!(present, Some(vec![7]));
    // Ordinary owned values use Clone without a codec or factory.
    let mut native = source.clone();
    native.get_mut("attributes").unwrap().clear();
    assert_eq!(source["attributes"], [1, 2]);
}

#[test]
fn deep_clone_contract_codec_errors() {
    // JSON cannot encode a sequence as an object key.
    let invalid = BTreeMap::from([(vec![1_u64], 2_u64)]);
    assert!(deep_clone_json(&invalid).is_err());
    assert!(deep_clone_json(&RejectDecode).is_err());
}

struct RejectDecode;

impl serde::Serialize for RejectDecode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("unreadable")
    }
}

impl<'de> serde::Deserialize<'de> for RejectDecode {
    fn deserialize<D: serde::Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Err(serde::de::Error::custom("cannot decode value"))
    }
}
