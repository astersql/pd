// Copyright 2026 AsterSQL.
use super::affinity_restore_test::fixture;
use crate::{
    manager::Manager,
    ranges::{GroupKeyRanges, get_label_rule_id},
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyutil::KeyRange;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{MemoryStore, MetaStore};
use std::sync::Arc;
fn op(id: &str, pairs: &[(u8, u8)]) -> GroupKeyRanges {
    GroupKeyRanges {
        group_id: id.into(),
        key_ranges: pairs
            .iter()
            .map(|(s, e)| KeyRange::new(vec![*s], vec![*e]))
            .collect(),
    }
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let p = ClusterPaths::new(35);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), p.clone(), 0)
        .await
        .unwrap();
    let mut m = Manager::default();
    m.create_affinity_groups(
        &ep,
        &p,
        &mut labels,
        &[op("a", &[]), op("b", &[(50, 60)])],
        1,
    )
    .await
    .unwrap();
    m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &[op("a", &[(1, 10)])], &[], 2)
        .await
        .unwrap();
    assert_eq!(m.groups["a"].info.range_count, 1);
    m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &[op("a", &[(20, 30)])], &[], 2)
        .await
        .unwrap();
    assert_eq!(m.groups["a"].info.range_count, 2);
    for (adds, removes) in [
        (vec![op("a", &[(1, 10)])], vec![]),
        (vec![op("a", &[(50, 60)])], vec![op("b", &[(50, 60)])]),
        (vec![op("a", &[(40, 45)])], vec![op("a", &[(1, 10)])]),
        (vec![], vec![op("a", &[(1, 9)])]),
        (vec![op("missing", &[])], vec![]),
        (vec![op("a", &[(80, 70)])], vec![]),
    ] {
        let before = m.key_ranges.clone();
        let v = m.groups["a"].affinity_version;
        assert!(
            m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &adds, &removes, 3)
                .await
                .is_err()
        );
        assert_eq!(m.key_ranges, before);
        assert_eq!(m.groups["a"].affinity_version, v);
    }
    // Cache miss must fall back to the actual label rule.
    m.key_ranges.remove("a");
    m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &[], &[op("a", &[(1, 10)])], 4)
        .await
        .unwrap();
    assert_eq!(m.key_ranges["a"], op("a", &[(20, 30)]));
    m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &[op("a", &[])], &[], 4)
        .await
        .unwrap();
    assert_eq!(m.groups["a"].info.range_count, 1);
    m.groups.get_mut("a").unwrap().info.region_count = 5;
    m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &[], &[op("a", &[(20, 30)])], 5)
        .await
        .unwrap();
    assert!(!m.key_ranges.contains_key("a"));
    assert_eq!(m.groups["a"].info.region_count, 0);
    assert!(labels.index().rule(&get_label_rule_id("a")).is_none());
    let restored_labels = RegionLabeler::load(StorageEndpoint::new(store), p.clone(), 0)
        .await
        .unwrap();
    let restored = Manager::initialize(&ep, &p, &restored_labels, 6)
        .await
        .unwrap();
    assert!(!restored.key_ranges.contains_key("a"));
    assert_eq!(restored.key_ranges["b"], op("b", &[(50, 60)]));
}
#[tokio::test]
async fn affinity_update_ranges_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn affinity_update_ranges_etcd() {
    let f = fixture::Fixture::start();
    contract(Arc::new(f.store().await)).await;
}

async fn fault_contract(inner: Arc<dyn MetaStore>) {
    use super::affinity_create_delete_test::Gate;
    use std::sync::atomic::Ordering;
    let gate = Gate::new(inner);
    let ep = StorageEndpoint::new(gate.clone());
    let p = ClusterPaths::new(351);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(gate.clone()), p.clone(), 0)
        .await
        .unwrap();
    let mut m = Manager::default();
    let initial = (0..121u16)
        .map(|i| GroupKeyRanges {
            group_id: format!("g{i:03}"),
            key_ranges: vec![],
        })
        .collect::<Vec<_>>();
    m.create_affinity_groups(&ep, &p, &mut labels, &initial, 1)
        .await
        .unwrap();
    let changes = initial
        .iter()
        .enumerate()
        .map(|(i, c)| GroupKeyRanges {
            group_id: c.group_id.clone(),
            key_ranges: vec![KeyRange::new(
                (i as u16 * 2).to_be_bytes().to_vec(),
                (i as u16 * 2 + 1).to_be_bytes().to_vec(),
            )],
        })
        .collect::<Vec<_>>();
    gate.writes.store(1, Ordering::SeqCst);
    assert!(
        m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &changes, &[], 2)
            .await
            .is_err()
    );
    assert!(m.key_ranges.is_empty());
    assert_eq!(labels.index().rules().count(), 0);
    let persisted = RegionLabeler::load(StorageEndpoint::new(gate.clone()), p.clone(), 0)
        .await
        .unwrap();
    assert_eq!(persisted.index().rules().count(), 0);
    gate.writes.store(2, Ordering::SeqCst);
    assert!(
        m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &changes, &[], 2)
            .await
            .is_err()
    );
    assert!(m.key_ranges.is_empty());
    assert_eq!(labels.index().rules().count(), 0);
    assert_eq!(m.groups["g000"].affinity_version, 2);
    let persisted = RegionLabeler::load(StorageEndpoint::new(gate.clone()), p.clone(), 0)
        .await
        .unwrap();
    assert_eq!(persisted.index().rules().count(), 120);
    gate.writes.store(-1, Ordering::SeqCst);
    m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &changes, &[], 3)
        .await
        .unwrap();
    gate.writes.store(1, Ordering::SeqCst);
    assert!(
        m.update_affinity_group_key_ranges(&ep, &p, &mut labels, &[], &changes, 4)
            .await
            .is_err()
    );
    assert_eq!(m.key_ranges.len(), 121);
    assert_eq!(labels.index().rules().count(), 121);
    let persisted = RegionLabeler::load(StorageEndpoint::new(gate), p, 0)
        .await
        .unwrap();
    assert_eq!(persisted.index().rules().count(), 121);
}
#[tokio::test]
async fn affinity_update_ranges_fault_memory() {
    fault_contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn affinity_update_ranges_fault_etcd() {
    let f = fixture::Fixture::start();
    fault_contract(Arc::new(f.store().await)).await;
}
