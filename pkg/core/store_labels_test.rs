// Copyright 2026 AsterSQL.
use crate::*;
use serde_json::Value;
fn labels(v: &Value) -> Vec<StoreLabel> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|l| StoreLabel {
            key: l[0].as_str().unwrap().into(),
            value: l[1].as_str().unwrap().into(),
        })
        .collect()
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("rust-fixtures/core_task_13.json")).unwrap()
}
#[test]
fn core_task_13_engine_fixture() {
    for s in fixture()["engine"].as_array().unwrap() {
        let store = StoreInfo::new(StoreMeta {
            labels: labels(&s["labels"]),
            ..Default::default()
        });
        assert_eq!(
            [
                store.is_tikv(),
                store.is_tiflash_write(),
                store.is_tiflash_compute()
            ],
            [
                s["flags"][0].as_bool().unwrap(),
                s["flags"][1].as_bool().unwrap(),
                s["flags"][2].as_bool().unwrap()
            ],
            "{}",
            s["name"]
        );
        assert_eq!(store.engine(), s["engine"].as_str().unwrap());
        let result = validate_store_engine_key(store.meta());
        assert_eq!(result.is_ok(), s["valid"].as_bool().unwrap());
        if let Err(e) = result {
            assert_eq!(
                e.to_string(),
                format!(
                    "invalid store engine label value: {:?}, valid values are \"tikv\", \"tiflash\", \"tiflash_compute\" or empty",
                    store.labels()[0].value
                )
            );
        }
    }
}
#[test]
fn core_task_13_merge_fixture() {
    for s in fixture()["merge"].as_array().unwrap() {
        let origin = labels(&s["origin"]);
        let before = origin.clone();
        let updates = labels(&s["updates"]);
        assert_eq!(
            merge_labels(&origin, &updates),
            labels(&s["expected"]),
            "{}",
            s["name"]
        );
        assert_eq!(origin, before);
    }
}
#[test]
fn core_task_13_lookup_and_absent_meta() {
    let mut store = StoreInfo::new(StoreMeta::default());
    store.set_labels(vec![
        StoreLabel {
            key: "Zone".into(),
            value: "first".into(),
        },
        StoreLabel {
            key: "zone".into(),
            value: "second".into(),
        },
    ]);
    assert_eq!(store.label_value("ZONE"), "first");
    assert_eq!(store.label_value("missing"), "");
    assert!(is_store_contain_label(store.meta(), "zone", "second"));
    assert!(is_store_contain_label(None, "missing", ""));
    let absent = StoreInfo::new(None);
    assert!(absent.labels().is_empty());
    assert!(absent.is_tikv());
    assert!(validate_store_engine_key(None).is_ok());
    store.set_labels(vec![]);
    assert!(store.labels().is_empty());
}

#[test]
fn core_task_13_unicode_and_error_escaping() {
    for (a, b, same) in [
        ("µ", "Μ", true),
        ("ϐ", "β", true),
        ("ı", "i", false),
        ("İ", "i", false),
        ("ß", "ss", false),
        ("ẞ", "ß", true),
    ] {
        let store = StoreInfo::new(StoreMeta {
            labels: vec![StoreLabel {
                key: a.into(),
                value: "v".into(),
            }],
            ..Default::default()
        });
        assert_eq!(store.label_value(b) == "v", same, "{a}/{b}");
    }
    let store = StoreMeta {
        labels: vec![StoreLabel {
            key: ENGINE_KEY.into(),
            value: "\u{1}\n\"\\".into(),
        }],
        ..Default::default()
    };
    assert_eq!(
        validate_store_engine_key(Some(&store))
            .unwrap_err()
            .to_string(),
        r#"invalid store engine label value: "\x01\n\"\\", valid values are "tikv", "tiflash", "tiflash_compute" or empty"#
    );
}

#[test]
fn core_task_13_go_simple_fold_orbits() {
    for pair in fixture()["simple_fold_orbits"].as_array().unwrap() {
        let a = pair[0].as_str().unwrap();
        let b = pair[1].as_str().unwrap();
        let store = StoreInfo::new(StoreMeta {
            labels: vec![StoreLabel {
                key: a.into(),
                value: "v".into(),
            }],
            ..Default::default()
        });
        assert_eq!(store.label_value(b), "v", "{a}/{b}");
    }
}
