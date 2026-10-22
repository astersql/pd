// Copyright 2026 AsterSQL.

use serde::Deserialize;

use crate::{
    REGION_INFLUENCE, SMALL_REGION_INFLUENCE, SMALL_REGION_THRESHOLD, STORE_LIMIT_TYPE_LEN,
    TYPE_NAME_VALUE, Type, UNLIMITED, VERSION_V1, VERSION_V2, type_from_name,
};

#[derive(Deserialize)]
struct Fixture {
    type_values: Vec<i64>,
    names: Vec<String>,
    expected: Expected,
}

#[derive(Deserialize)]
struct Expected {
    type_names: Vec<String>,
    lookups: Vec<Lookup>,
    type_count: usize,
    region_influence: Vec<i64>,
    small_region_influence: Vec<i64>,
    small_region_threshold: i64,
    unlimited: f64,
    versions: Vec<String>,
}

#[derive(Deserialize)]
struct Lookup {
    value: i64,
    found: bool,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("rust-fixtures/core_task_02.json")).unwrap()
}

#[test]
fn core_task_02_type_values_and_names() {
    let f = fixture();
    assert_eq!(f.type_values.len(), f.expected.type_names.len());
    for (value, name) in f.type_values.iter().zip(&f.expected.type_names) {
        let typ = Type::from_i64(*value);
        assert_eq!(typ.as_i64(), *value);
        assert_eq!(typ.to_string(), *name);
    }
    let known = [
        Type::ADD_PEER,
        Type::REMOVE_PEER,
        Type::SEND_SNAPSHOT,
        Type::TRANSFER_LEADER_IN,
    ];
    assert_eq!(known.len(), f.expected.type_count);
    assert_eq!(TYPE_NAME_VALUE.len(), known.len());
    for (i, typ) in known.into_iter().enumerate() {
        assert_eq!(typ.as_i64(), f.type_values[i]);
        assert_eq!(type_from_name(&typ.to_string()), Some(typ));
        assert!(TYPE_NAME_VALUE.contains(&(f.expected.type_names[i].as_str(), typ)));
    }
    assert_eq!(Type::default(), Type::ADD_PEER);
    assert_eq!(Type::from_i64(i64::MIN).to_string(), "");
}

#[test]
fn core_task_02_name_lookup_and_unknowns() {
    let f = fixture();
    assert_eq!(f.names.len(), f.expected.lookups.len());
    for (name, expected) in f.names.iter().zip(&f.expected.lookups) {
        let result = type_from_name(name);
        assert_eq!(result.is_some(), expected.found, "{name:?}");
        // Go's missing map lookup returns its zero value alongside false.
        assert_eq!(
            result.unwrap_or_default().as_i64(),
            expected.value,
            "{name:?}"
        );
    }
}

#[test]
fn core_task_02_influence_and_version_constants() {
    let f = fixture().expected;
    assert_eq!(STORE_LIMIT_TYPE_LEN, f.type_count);
    assert_eq!(REGION_INFLUENCE.as_slice(), f.region_influence);
    assert_eq!(SMALL_REGION_INFLUENCE.as_slice(), f.small_region_influence);
    assert_eq!(SMALL_REGION_THRESHOLD, f.small_region_threshold);
    assert_eq!(UNLIMITED, f.unlimited);
    assert_eq!([VERSION_V1, VERSION_V2], f.versions.as_slice());
    assert_eq!(
        REGION_INFLUENCE[Type::TRANSFER_LEADER_IN.as_i64() as usize],
        1000
    );
    assert_eq!(
        SMALL_REGION_INFLUENCE.get(Type::SEND_SNAPSHOT.as_i64() as usize),
        None
    );
    assert_eq!(
        SMALL_REGION_INFLUENCE.get(Type::TRANSFER_LEADER_IN.as_i64() as usize),
        None
    );
}
