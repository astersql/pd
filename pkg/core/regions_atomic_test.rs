// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::{Arc, Mutex};
fn region(id: u64, start: &str, end: &str, version: u64) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.into(),
                end_key: end.into(),
            },
            epoch: Some(RegionEpoch {
                version,
                conf_ver: 2,
            }),
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ))
}
#[test]
fn core_task_46_fixture_all_entries() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_46.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        for entry in 0..3 {
            let mut r = RegionsInfo::new();
            r.set_region(region(1, "a", "m", 4));
            r.set_region(region(2, "m", "z", 4));
            let incoming = region(
                case["id"].as_u64().unwrap(),
                case["start"].as_str().unwrap(),
                case["end"].as_str().unwrap(),
                case["version"].as_u64().unwrap(),
            );
            let before = r.get_regions();
            let rejected = case["rejected"].as_bool().unwrap();
            let overlaps = match entry {
                0 => {
                    let result = r.check_and_put_root_tree(incoming.clone());
                    assert_eq!(result.is_err(), rejected);
                    result.unwrap_or_default()
                }
                1 => {
                    let result = r.atomic_check_and_put_region(incoming.clone());
                    assert_eq!(result.is_err(), rejected);
                    result.unwrap_or_default()
                }
                _ => {
                    let result = r.check_and_put_region(incoming.clone());
                    if rejected {
                        assert_eq!(result.len(), 1);
                        assert!(Arc::ptr_eq(&result[0], &incoming));
                        vec![]
                    } else {
                        result
                    }
                }
            };
            let expected: Vec<u64> = serde_json::from_value(case["overlaps"].clone()).unwrap();
            assert_eq!(
                overlaps.iter().map(|r| r.meta().id).collect::<Vec<_>>(),
                expected,
                "{}",
                case["name"]
            );
            if rejected {
                for old in before {
                    assert!(Arc::ptr_eq(&old, &r.get_region(old.meta().id).unwrap()));
                }
                assert_eq!(r.tree_len(), 2);
            } else {
                assert!(Arc::ptr_eq(
                    &incoming,
                    &r.get_region(incoming.meta().id).unwrap()
                ));
                assert_eq!(r.tree_len(), case["len"].as_u64().unwrap() as usize);
            }
        }
    }
}
#[test]
fn core_task_46_precheck_snapshot_cannot_authorize_stale_commit() {
    let mut r = RegionsInfo::new();
    let old = region(1, "a", "z", 2);
    assert!(r.pre_check_put_region(&old).2.is_ok());
    r.check_and_put_root_tree(region(1, "a", "z", 5)).unwrap();
    assert!(r.atomic_check_and_put_region(old).is_err());
    assert_eq!(r.get_region(1).unwrap().meta().epoch.unwrap().version, 5);
}
#[test]
fn core_task_46_shared_owners_preserve_newest_version() {
    let registry = Arc::new(Mutex::new(RegionsInfo::new()));
    std::thread::scope(|scope| {
        for version in 2..34 {
            let registry = registry.clone();
            scope.spawn(move || {
                let _ = registry
                    .lock()
                    .unwrap()
                    .atomic_check_and_put_region(region(1, "a", "z", version));
            });
        }
    });
    assert_eq!(
        registry
            .lock()
            .unwrap()
            .get_region(1)
            .unwrap()
            .meta()
            .epoch
            .unwrap()
            .version,
        33
    );
}

#[test]
fn core_task_68_combined_put_updates_roles_but_root_only_does_not() {
    let mut regions = RegionsInfo::new();
    let peer = crate::Peer {
        id: 1,
        store_id: 7,
        ..Default::default()
    };
    let region = Arc::new(RegionInfo::new(
        crate::RegionMeta {
            id: 1,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    ));
    regions.atomic_check_and_put_region(region.clone()).unwrap();
    assert_eq!(regions.store_role_count(7, crate::RegionRole::Leader), 1);
    regions.reset_region_cache();
    regions.check_and_put_region(region.clone());
    assert_eq!(regions.store_role_count(7, crate::RegionRole::Leader), 1);
    regions.reset_region_cache();
    regions.put_region(region.clone());
    assert_eq!(regions.store_role_count(7, crate::RegionRole::Leader), 1);
    regions.reset_region_cache();
    regions.check_and_put_root_tree(region).unwrap();
    assert_eq!(regions.store_role_count(7, crate::RegionRole::Leader), 0);
}
