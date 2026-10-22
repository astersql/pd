// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn region(v: &serde_json::Value) -> Arc<RegionInfo> {
    let mut r = RegionInfo::new(
        RegionMeta {
            id: v[0].as_u64().unwrap(),
            key_range: KeyRange {
                start_key: v[1].as_str().unwrap().as_bytes().to_vec(),
                end_key: v[2].as_str().unwrap().as_bytes().to_vec(),
            },
            epoch: if v[3].is_null() {
                None
            } else {
                Some(RegionEpoch {
                    version: v[3].as_u64().unwrap(),
                    conf_ver: v[4].as_u64().unwrap(),
                })
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    r.term = v[5].as_u64().unwrap();
    Arc::new(r)
}
#[test]
fn core_task_45_fixture_checks() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_45.json")).unwrap();
    for c in f["cases"].as_array().unwrap() {
        let mut registry = RegionsInfo::new();
        for old in c["stored"].as_array().unwrap() {
            registry.set_region(region(old));
        }
        let incoming = region(&c["incoming"]);
        let before = registry.get_regions();
        let (origin, overlaps, result) = registry.pre_check_put_region(&incoming);
        assert_eq!(
            origin.as_ref().map(|r| r.meta().id),
            c["origin"].as_u64(),
            "{}",
            c["name"]
        );
        assert_eq!(
            overlaps.iter().map(|r| r.meta().id).collect::<Vec<_>>(),
            serde_json::from_value::<Vec<u64>>(c["overlaps"].clone()).unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(
            result.as_ref().err().map(|e| e.current.id),
            c["stale"].as_u64(),
            "{}",
            c["name"]
        );
        if let Err(e) = result {
            assert_eq!(e.incoming, *incoming.meta());
            assert!(e.to_string().starts_with("region is stale"));
        }
        let valid = registry.valid_region(incoming.meta());
        let kind = match valid {
            Ok(()) => "ok",
            Err(RegionValidationError::NotFound { .. }) => "not_found",
            Err(RegionValidationError::InvalidEpoch { .. }) => "invalid_epoch",
        };
        assert_eq!(kind, c["valid"].as_str().unwrap(), "{}", c["name"]);
        assert_eq!(registry.tree_len(), before.len());
        for old in before {
            assert!(Arc::ptr_eq(
                &old,
                &registry.get_region(old.meta().id).unwrap()
            ));
        }
    }
}

#[test]
fn core_task_45_error_precedence_and_payloads() {
    let incoming = region(&serde_json::json!([1, "a", "z", 2, 2, 1]));
    let origin = region(&serde_json::json!([1, "a", "m", 9, 9, 9]));
    let overlap = region(&serde_json::json!([2, "m", "z", 3, 0, 0]));
    let error = check_region(&incoming, Some(&origin), &[overlap.clone()]).unwrap_err();
    assert_eq!(error.current, *overlap.meta());
    assert_eq!(error.incoming, *incoming.meta());
    let mut registry = RegionsInfo::new();
    registry.set_region(origin.clone());
    let error = registry.valid_region(incoming.meta()).unwrap_err();
    assert_eq!(
        error,
        RegionValidationError::InvalidEpoch {
            request: incoming.meta().epoch.unwrap(),
            current: origin.meta().epoch.unwrap(),
        }
    );
    assert!(
        error
            .to_string()
            .starts_with("invalid region epoch, request:")
    );
    let missing = region(&serde_json::json!([3, "z", "", 0, 0, 0]));
    let error = registry.valid_region(missing.meta()).unwrap_err();
    assert_eq!(
        error,
        RegionValidationError::NotFound {
            request: missing.meta().clone()
        }
    );
    assert!(
        error
            .to_string()
            .starts_with("region not found, request region:")
    );
}

#[test]
fn core_task_45_nil_epoch_and_unsigned_limits() {
    let old = region(&serde_json::json!([1, "a", "", null, null, u64::MAX]));
    let zero = region(&serde_json::json!([1, "a", "", 0, 0, 0]));
    let mut registry = RegionsInfo::new();
    registry.set_region(old);
    assert!(registry.pre_check_put_region(&zero).2.is_ok());
    assert!(registry.valid_region(zero.meta()).is_ok());
    let maximal = region(&serde_json::json!([
        1,
        "a",
        "",
        u64::MAX,
        u64::MAX,
        u64::MAX
    ]));
    assert!(registry.pre_check_put_region(&maximal).2.is_ok());
    registry.set_region(maximal);
    assert!(registry.pre_check_put_region(&zero).2.is_err());
    assert!(registry.valid_region(zero.meta()).is_err());
    assert!(!zero.is_region_recreated());
}
