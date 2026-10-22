// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreMeta, StoresInfo};
use std::sync::Arc;
fn store(id: u64) -> Arc<StoreInfo> {
    Arc::new(StoreInfo::new(StoreMeta {
        id,
        ..Default::default()
    }))
}
#[test]
fn core_task_26_registry_snapshots() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_26.json")).unwrap();
    assert_eq!(fixture["empty_count"], 0);
    let registry = StoresInfo::default();
    assert_eq!(registry.store_count(), 0);
    assert!(registry.get_store(1).is_none());
    let first = store(1);
    registry.put_store(first.clone());
    assert!(Arc::ptr_eq(&first, &registry.get_store(1).unwrap()));
    let snapshot = registry.get_stores();
    let replacement = store(1);
    registry.put_store(replacement.clone());
    assert!(Arc::ptr_eq(&replacement, &registry.get_store(1).unwrap()));
    assert!(Arc::ptr_eq(&snapshot[0], &first));
    registry.put_store(store(2));
    let mut ids = registry.get_store_ids();
    ids.sort();
    assert_eq!(
        serde_json::to_value(&ids).unwrap(),
        fixture["ids_after_insert"]
    );
    assert_eq!(registry.get_meta_stores().len(), 2);
    registry.delete_store(1);
    registry.delete_store(99);
    assert!(!registry.patch_store(1, |_| panic!("removed patch must not run")));
    assert!(registry.get_store(1).is_none());
    registry.reset_stores();
    assert!(registry.get_stores().is_empty());
    assert!(registry.get_store_ids().is_empty());
    assert!(registry.get_meta_stores().is_empty());
    assert_eq!(snapshot[0].id(), 1);
}
#[test]
fn core_task_26_patch_current_and_concurrent_updates() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_26.json")).unwrap();
    let registry = Arc::new(StoresInfo::default());
    registry.put_store(store(1));
    let old = registry.get_store(1).unwrap();
    let workers: Vec<_> = (0..fixture["workers"].as_u64().unwrap())
        .map(|_| {
            let registry = registry.clone();
            let patches = fixture["patches_per_worker"].as_u64().unwrap();
            std::thread::spawn(move || {
                for _ in 0..patches {
                    assert!(registry.patch_store(1, |s| s.leader_count += 1));
                    assert_eq!(registry.get_stores().len(), 1);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        registry.get_store(1).unwrap().leader_count,
        fixture["expected_leader_count"].as_i64().unwrap()
    );
    assert_eq!(old.leader_count, 0);
    registry.put_store(store(2));
    assert!(registry.patch_store(1, |s| s.region_count = 7));
    assert_eq!(
        registry.get_store(1).unwrap().leader_count,
        fixture["expected_leader_count"].as_i64().unwrap()
    );
    assert_eq!(registry.get_store(1).unwrap().region_count, 7);
    assert_eq!(registry.get_store(2).unwrap().leader_count, 0);
}

#[test]
fn core_task_26_zero_id_and_absent_metadata() {
    let registry = StoresInfo::new();
    registry.put_store(Arc::new(StoreInfo::new(None)));
    assert_eq!(registry.get_store_ids(), vec![0]);
    assert_eq!(registry.get_meta_stores(), vec![None]);
    assert!(registry.patch_store(0, |s| s.region_count = 3));
    assert_eq!(registry.get_store(0).unwrap().region_count, 3);
    assert!(registry.get_meta_stores()[0].is_none());
    registry.reset_stores();
    assert!(!registry.patch_store(0, |_| panic!("reset entries cannot be patched")));
}
