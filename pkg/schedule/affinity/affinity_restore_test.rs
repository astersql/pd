// Copyright 2026 AsterSQL.
use crate::{
    group::{Availability, Group},
    manager::Manager,
    ranges::{GroupKeyRanges, make_label_rule},
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyutil::KeyRange;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_endpoint::{EndpointError, StorageEndpoint};
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::sync::Arc;
#[path = "../../storage/kv/etcd_fixture_test.rs"]
mod fixture;

async fn seed(store: Arc<dyn MetaStore>) -> (Arc<dyn MetaStore>, ClusterPaths) {
    let ep = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(33);
    ep.run_in_txn(async |txn| {
        for id in ["persist", "empty"] {
            ep.save_affinity_group(
                txn,
                &paths,
                id,
                &Group {
                    id: id.into(),
                    create_timestamp: 123,
                    leader_store_id: 1,
                    voter_store_ids: vec![1, 2],
                    ..Default::default()
                },
            )?;
        }
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    // Physical suffix is not authoritative; duplicate logical IDs retain the first.
    Base::new(store.as_ref())
        .save(
            paths.affinity_group_path("z-duplicate").as_bytes(),
            br#"{"id":"persist","leader_store_id":99}"#,
        )
        .await
        .unwrap();
    Base::new(store.as_ref())
        .save(paths.affinity_group_path("broken").as_bytes(), b"{")
        .await
        .unwrap();
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
        .await
        .unwrap();
    for id in ["persist", "ghost"] {
        let rule = make_label_rule(&GroupKeyRanges {
            group_id: id.into(),
            key_ranges: vec![KeyRange::new(vec![1], vec![2])],
        });
        labels.set_label_rule(rule, 0).await.unwrap();
    }
    (store, paths)
}
async fn verify(store: Arc<dyn MetaStore>, paths: &ClusterPaths) {
    let ep = StorageEndpoint::new(store.clone());
    let labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
        .await
        .unwrap();
    let manager = Manager::initialize(&ep, paths, &labels, 100).await.unwrap();
    assert_eq!(manager.groups.len(), 2);
    assert!(!manager.groups.contains_key("ghost"));
    let g = &manager.groups["persist"];
    assert_eq!(g.info.group.leader_store_id, 1);
    assert_eq!(g.info.group.create_timestamp, 123);
    assert_eq!(g.info.group.voter_store_ids, vec![1, 2]);
    assert_eq!(g.info.range_count, 1);
    assert_eq!(g.affinity_version, 2);
    assert_eq!(g.info.availability(700), Availability::Degraded);
    assert_eq!(g.info.availability(701), Availability::Expired);
    assert_eq!(g.info.region_count, 0);
    assert_eq!(manager.groups["empty"].affinity_version, 1);
    assert_eq!(manager.key_ranges.len(), 1);
    assert!(g.label_rule.is_some());
    assert!(
        !ep.load_affinity_group(paths, "broken")
            .await
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn affinity_restore_memory() {
    let (store, p) = seed(Arc::new(MemoryStore::default())).await;
    verify(store.clone(), &p).await;
}
#[tokio::test]
async fn affinity_restore_etcd_wal() {
    let mut f = fixture::Fixture::start();
    let (store, p) = seed(Arc::new(f.store().await)).await;
    verify(store.clone(), &p).await;
    drop(store);
    f.command("stop", "stopped");
    f.command("start", "restarted");
    verify(Arc::new(f.store().await), &p).await;
}
#[tokio::test]
async fn affinity_restore_overlap_rejected() {
    let (store, p) = seed(Arc::new(MemoryStore::default())).await;
    let ep = StorageEndpoint::new(store.clone());
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), p.clone(), 0)
        .await
        .unwrap();
    labels
        .set_label_rule(
            make_label_rule(&GroupKeyRanges {
                group_id: "empty".into(),
                key_ranges: vec![KeyRange::new(vec![1], vec![3])],
            }),
            0,
        )
        .await
        .unwrap();
    let err = Manager::initialize(&ep, &p, &labels, 100)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("key range overlaps between groups")
    );
}
#[tokio::test]
async fn affinity_restore_unadjusted_rule_skipped() {
    let (store, p) = seed(Arc::new(MemoryStore::default())).await;
    let ep = StorageEndpoint::new(store.clone());
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), p.clone(), 0)
        .await
        .unwrap();
    // Watcher/validated entry preserves unadjusted data; restore skips extraction errors.
    labels
        .set_validated_label_rule(make_label_rule(&GroupKeyRanges {
            group_id: "persist".into(),
            key_ranges: vec![KeyRange::new(vec![1], vec![2])],
        }))
        .await
        .unwrap();
    let m = Manager::initialize(&ep, &p, &labels, 100).await.unwrap();
    assert_eq!(m.groups.len(), 2);
    assert_eq!(m.groups["persist"].affinity_version, 1);
    assert!(m.key_ranges.is_empty());
}

#[tokio::test]
async fn affinity_restore_null_and_empty_rules() {
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let ep = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(33);
    Base::new(store.as_ref())
        .save(paths.affinity_group_path("physical").as_bytes(), b"null")
        .await
        .unwrap();
    ep.run_in_txn(async |txn| {
        ep.save_affinity_group(
            txn,
            &paths,
            "empty",
            &Group {
                id: "empty".into(),
                ..Default::default()
            },
        )
    })
    .await
    .unwrap();
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store), paths.clone(), 0)
        .await
        .unwrap();
    labels
        .set_validated_label_rule(make_label_rule(&GroupKeyRanges {
            group_id: "empty".into(),
            key_ranges: vec![],
        }))
        .await
        .unwrap();
    let restored = Manager::initialize(&ep, &paths, &labels, 100)
        .await
        .unwrap();
    assert_eq!(restored.groups.len(), 2);
    assert_eq!(restored.groups[""].info.group, Group::default());
    assert!(restored.groups["empty"].label_rule.is_some());
    assert_eq!(restored.groups["empty"].info.range_count, 0);
    assert_eq!(restored.groups["empty"].affinity_version, 2);
    assert!(restored.key_ranges.is_empty());
}

#[tokio::test]
async fn affinity_restore_storage_failure() {
    let mut f = fixture::Fixture::start();
    let (store, paths) = seed(Arc::new(f.store().await)).await;
    let labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
        .await
        .unwrap();
    f.command("stop", "stopped");
    let failure = Manager::initialize(&StorageEndpoint::new(store), &paths, &labels, 100)
        .await
        .unwrap_err();
    assert!(matches!(failure, crate::manager::RestoreError::Storage(_)));
}
