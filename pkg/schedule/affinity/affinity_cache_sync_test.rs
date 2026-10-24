// Copyright 2026 AsterSQL.
use crate::{
    group::Group,
    manager::Manager,
    ranges::{GroupKeyRanges, get_label_rule_id, make_label_rule},
};
use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyutil::KeyRange;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{MemoryStore, MetaStore};
use std::sync::Arc;
fn region(id: u64, store: u64, start: u8, end: u8) -> Arc<RegionInfo> {
    let p = Peer {
        id: store,
        store_id: store,
        ..Default::default()
    };
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: astersql_pd_core::KeyRange {
                start_key: vec![start],
                end_key: vec![end],
            },
            peers: vec![p.clone()],
            ..Default::default()
        },
        Some(p),
        vec![],
        vec![],
    ))
}
fn counts(m: &Manager, regions: usize, affinity: usize) {
    assert_eq!(m.groups["g"].info.region_count, regions);
    assert_eq!(m.groups["g"].info.affinity_region_count, affinity);
    assert_eq!(m.affinity_region_count(), affinity);
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(38);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store), paths.clone(), 0)
        .await
        .unwrap();
    let ranges = GroupKeyRanges {
        group_id: "g".into(),
        key_ranges: vec![KeyRange::new(vec![1], vec![5])],
    };
    let mut rule = make_label_rule(&ranges);
    rule.check_and_adjust(0).unwrap();
    let group = Group {
        id: "g".into(),
        create_timestamp: 100,
        leader_store_id: 1,
        voter_store_ids: vec![1],
    };
    let mut m = Manager::default();
    // Label precedes both group and labeler. Buffer attachment must preserve range count.
    m.sync_key_ranges_from_etcd(&rule).unwrap();
    assert!(m.key_ranges.is_empty());
    m.sync_group_from_etcd(&group, &labels, 100);
    assert_eq!(m.groups["g"].info.range_count, 1);
    assert!(m.label_rule_buffer.is_empty());
    labels.set_label_rule(rule.clone(), 0).await.unwrap();
    m.restore_affinity_group("g", 100);
    let r = region(10, 1, 1, 2);
    assert!(m.get_region_affinity_group_state(&labels, Some(&r), 100).1);
    counts(&m, 0, 0);
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, Some(&r), 100)
            .1
    );
    counts(&m, 1, 1);
    // Repeated identity, a cloned snapshot (epoch/peer updates), and no-match reads.
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, Some(&r), 100)
            .1
    );
    counts(&m, 1, 1);
    let mut epoch_meta = r.meta().clone();
    epoch_meta.epoch = Some(astersql_pd_core::RegionEpoch {
        conf_ver: 9,
        version: 9,
    });
    epoch_meta.peers.push(Peer {
        id: 99,
        store_id: 99,
        role: astersql_pd_core::PeerRole::Learner,
        ..Default::default()
    });
    let epoch = Arc::new(RegionInfo::new(
        epoch_meta,
        r.leader().cloned(),
        vec![],
        vec![],
    ));
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, Some(&epoch), 100)
            .1
    );
    counts(&m, 1, 1);
    // Go processes arrival order; it does not reject a lower region epoch here.
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, Some(&r), 100)
            .1
    );
    counts(&m, 1, 1);
    let changed = region(10, 2, 1, 2);
    assert!(
        !m.get_region_affinity_group_state(&labels, Some(&changed), 100)
            .1
    );
    counts(&m, 1, 1);
    assert!(
        !m.get_and_cache_region_affinity_group_state(&labels, Some(&changed), 100)
            .1
    );
    counts(&m, 1, 0);
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, Some(&r), 100)
            .1
    );
    let outside = region(10, 1, 0, 6);
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, Some(&outside), 100)
            .0
            .is_none()
    );
    counts(&m, 1, 1); // Missing ownership deliberately retains the old entry (Go behavior).
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, None, 100)
            .0
            .is_none()
    );
    let version = m.groups["g"].affinity_version;
    let mut same = group.clone();
    same.create_timestamp = 900;
    m.sync_group_from_etcd(&same, &labels, 100);
    assert_eq!(m.groups["g"].affinity_version, version);
    assert_eq!(m.groups["g"].info.group.create_timestamp, 100);
    let mut other = group.clone();
    other.leader_store_id = 2;
    other.voter_store_ids = vec![2];
    m.sync_group_from_etcd(&other, &labels, 100);
    counts(&m, 1, 0);
    // Removing stale affinity must not underflow current counts.
    m.invalid_cache(10);
    m.invalid_cache(10);
    counts(&m, 0, 0);
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, Some(&changed), 100)
            .1
    );
    counts(&m, 1, 1);
    m.degrade_affinity_group("g", 100);
    counts(&m, 1, 0);
    assert!(
        !m.get_and_cache_region_affinity_group_state(&labels, Some(&changed), 100)
            .1
    );
    counts(&m, 1, 0);
    m.restore_affinity_group("g", 100);
    m.get_and_cache_region_affinity_group_state(&labels, Some(&changed), 100);
    counts(&m, 1, 1);
    m.sync_key_ranges_from_etcd(&rule).unwrap();
    counts(&m, 1, 0);
    m.get_and_cache_region_affinity_group_state(&labels, Some(&changed), 100);
    counts(&m, 1, 1);
    let mut bad = rule.clone();
    bad.data = serde_json::json!([{"start_key":"xx","end_key":"05"}]);
    let bad = astersql_pd_schedule_labeler::LabelRule::from_json_without_check(
        &serde_json::to_vec(&bad).unwrap(),
    )
    .unwrap();
    let version = m.groups["g"].affinity_version;
    assert!(m.sync_key_ranges_from_etcd(&bad).is_err());
    assert_eq!(m.groups["g"].affinity_version, version);
    counts(&m, 1, 1);
    m.sync_key_ranges_delete_from_etcd("unrelated");
    counts(&m, 1, 1);
    m.sync_key_ranges_delete_from_etcd(&get_label_rule_id("g"));
    counts(&m, 0, 0);
    assert!(!m.key_ranges.contains_key("g"));
    // Manager sync does not publish labels; host supplies a separate labeler event.
    labels
        .delete_label_rule(&get_label_rule_id("g"))
        .await
        .unwrap();
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, Some(&r), 100)
            .0
            .is_none()
    );
    labels.set_label_rule(rule.clone(), 0).await.unwrap();
    m.sync_key_ranges_from_etcd(&rule).unwrap();
    m.get_and_cache_region_affinity_group_state(&labels, Some(&changed), 100);
    counts(&m, 1, 1);
    m.sync_group_delete_from_etcd("g");
    assert!(m.regions.is_empty());
    assert_eq!(m.affinity_region_count(), 0);
    m.sync_group_delete_from_etcd("g");
    // Existing labeler takes precedence on recreation; new group starts degraded.
    m.sync_group_from_etcd(&group, &labels, 100);
    assert_eq!(m.groups["g"].info.range_count, 1);
    assert!(
        !m.get_and_cache_region_affinity_group_state(&labels, Some(&r), 100)
            .1
    );
    counts(&m, 1, 0);
    // Local deletion shares cache cleanup; local range removals also clear entries.
    m.update_affinity_group_key_ranges(&ep, &paths, &mut labels, &[], &[ranges.clone()], 100)
        .await
        .unwrap();
    counts(&m, 0, 0);
    assert!(m.regions.is_empty());
    m.update_affinity_group_key_ranges(&ep, &paths, &mut labels, &[ranges], &[], 100)
        .await
        .unwrap();
    m.get_and_cache_region_affinity_group_state(&labels, Some(&r), 100);
    counts(&m, 1, 0);
    m.delete_affinity_groups(&ep, &paths, &mut labels, &["g".into()], true)
        .await
        .unwrap();
    assert!(m.regions.is_empty());
}
#[tokio::test]
async fn affinity_cache_sync_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn affinity_cache_sync_etcd() {
    let f = super::affinity_restore_test::fixture::Fixture::start();
    contract(Arc::new(f.store().await)).await;
}

#[tokio::test]
async fn affinity_cache_sync_buffer_and_reassignment() {
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store), ClusterPaths::new(380), 0)
        .await
        .unwrap();
    let mut m = Manager::default();
    let mut rule = make_label_rule(&GroupKeyRanges {
        group_id: "g".into(),
        key_ranges: vec![KeyRange::new([1], [5])],
    });
    rule.check_and_adjust(0).unwrap();
    m.sync_key_ranges_from_etcd(&rule).unwrap();
    let mut empty = rule.clone();
    empty.data = serde_json::Value::Null;
    m.sync_key_ranges_from_etcd(&empty).unwrap();
    assert!(m.label_rule_buffer.is_empty());
    m.sync_key_ranges_from_etcd(&rule).unwrap();
    m.sync_key_ranges_delete_from_etcd(&rule.id);
    assert!(m.label_rule_buffer.is_empty());
    m.sync_key_ranges_from_etcd(&rule).unwrap();
    m.sync_group_delete_from_etcd("g");
    assert!(m.label_rule_buffer.is_empty());
    let group = Group {
        id: "g".into(),
        leader_store_id: 1,
        voter_store_ids: vec![1],
        ..Default::default()
    };
    m.sync_group_from_etcd(&group, &labels, 100);
    assert_eq!(m.groups["g"].info.range_count, 0);
    m.sync_key_ranges_from_etcd(&rule).unwrap();
    labels.set_label_rule(rule.clone(), 0).await.unwrap();
    m.restore_affinity_group("g", 100);
    let r = region(1, 1, 1, 2);
    assert!(
        m.get_and_cache_region_affinity_group_state(&labels, Some(&r), 100)
            .1
    );
    counts(&m, 1, 1);
    // Empty updates reset affinity but preserve membership, unlike deletion events.
    m.sync_key_ranges_from_etcd(&empty).unwrap();
    counts(&m, 1, 0);
    assert!(!m.key_ranges.contains_key("g"));
    m.sync_key_ranges_from_etcd(&rule).unwrap();
    // Publish a higher-priority rule for h, then query a fresh snapshot with the same ID.
    let h = Group {
        id: "h".into(),
        ..group.clone()
    };
    m.sync_group_from_etcd(&h, &labels, 100);
    let mut h_rule = make_label_rule(&GroupKeyRanges {
        group_id: "h".into(),
        key_ranges: vec![KeyRange::new([1], [5])],
    });
    h_rule.index = 10;
    h_rule.check_and_adjust(0).unwrap();
    m.sync_key_ranges_from_etcd(&h_rule).unwrap();
    labels.set_label_rule(h_rule, 0).await.unwrap();
    m.restore_affinity_group("h", 100);
    let fresh = region(1, 1, 1, 2);
    let result = m.get_and_cache_region_affinity_group_state(&labels, Some(&fresh), 100);
    assert_eq!(result.0.unwrap().group.id, "h");
    assert!(result.1);
    assert_eq!(m.groups["g"].info.region_count, 0);
    assert_eq!(m.groups["h"].info.region_count, 1);
    assert_eq!(m.affinity_region_count(), 1);
    m.invalid_cache(1);
    m.invalid_cache(1);
    assert_eq!(m.affinity_region_count(), 0);
}
