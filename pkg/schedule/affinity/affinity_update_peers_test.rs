// Copyright 2026 AsterSQL.
use super::affinity_restore_test::fixture;
use crate::{group::Group, manager::Manager, ranges::GroupKeyRanges};
use astersql_pd_core::{Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{MemoryStore, MetaStore};
use std::sync::Arc;
fn stores() -> Vec<StoreInfo> {
    (1..=3)
        .map(|id| {
            StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            })
        })
        .collect()
}
fn region(id: u64) -> RegionInfo {
    let p = Peer {
        id,
        store_id: id,
        ..Default::default()
    };
    RegionInfo::new(
        RegionMeta {
            id,
            peers: vec![
                p.clone(),
                Peer {
                    id: 99,
                    store_id: 99,
                    role: astersql_pd_core::PeerRole::Learner,
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
        Some(p),
        vec![],
        vec![],
    )
}
async fn contract(inner: Arc<dyn MetaStore>) {
    use super::affinity_create_delete_test::Gate;
    use std::sync::atomic::Ordering;
    let gate = Gate::new(inner);
    let ep = StorageEndpoint::new(gate.clone());
    let paths = ClusterPaths::new(37);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(gate.clone()), paths.clone(), 0)
        .await
        .unwrap();
    let mut m = Manager::default();
    m.create_affinity_groups(
        &ep,
        &paths,
        &mut labels,
        &[GroupKeyRanges {
            group_id: "g".into(),
            key_ranges: vec![],
        }],
        100,
    )
    .await
    .unwrap();
    let ss = stores();
    assert!(m.group_observation("missing", 100).is_none());
    assert!(
        m.update_affinity_group_peers(&ep, &paths, &ss, "bad/id", 1, &[1], 100)
            .await
            .unwrap_err()
            .to_string()
            .contains("ErrInvalidGroupID")
    );
    let token = m.group_observation("g", 100).unwrap();
    let no_leader = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    m.observe_available_region(&ep, &paths, &ss, &no_leader, Some(&token), 100)
        .await;
    assert_eq!(m.groups["g"].affinity_version, token.affinity_version);
    m.observe_available_region(&ep, &paths, &ss, &region(1), Some(&token), 100)
        .await;
    assert_eq!(m.groups["g"].info.group.voter_store_ids, vec![1]);
    assert_eq!(m.groups["g"].affinity_version, token.affinity_version + 1);
    m.observe_available_region(&ep, &paths, &ss, &region(2), Some(&token), 100)
        .await;
    assert_eq!(m.groups["g"].info.group.voter_store_ids, vec![1]);
    // Expiration permits a new layout, so only the stale version prevents this write.
    m.expire_affinity_group("g", 100);
    let version = m.groups["g"].affinity_version;
    gate.writes.store(1, Ordering::SeqCst);
    m.observe_available_region(&ep, &paths, &ss, &region(2), Some(&token), 100)
        .await;
    assert_eq!(m.groups["g"].info.group.voter_store_ids, vec![1]);
    assert_eq!(m.groups["g"].affinity_version, version);
    assert_eq!(
        gate.writes.load(Ordering::SeqCst),
        1,
        "stale observation must not reach storage"
    );
    gate.writes.store(-1, Ordering::SeqCst);
    m.restore_affinity_group("g", 100);
    let current = m.group_observation("g", 100).unwrap();
    m.observe_available_region(&ep, &paths, &ss, &region(2), Some(&current), 100)
        .await;
    assert_eq!(m.groups["g"].info.group.voter_store_ids, vec![1]);
    m.degrade_affinity_group("g", 100);
    let degraded = m.group_observation("g", 100).unwrap();
    m.observe_available_region(&ep, &paths, &ss, &region(2), Some(&degraded), 100)
        .await;
    assert_eq!(m.groups["g"].affinity_version, degraded.affinity_version);
    m.observe_available_region(&ep, &paths, &ss, &region(1), Some(&degraded), 100)
        .await;
    assert!(
        m.group_observation("g", 100)
            .unwrap()
            .state
            .affinity_scheduling_allowed
    );
    m.expire_affinity_group("g", 100);
    let expired = m.group_observation("g", 100).unwrap();
    m.observe_available_region(&ep, &paths, &ss, &region(2), Some(&expired), 100)
        .await;
    assert_eq!(m.groups["g"].info.group.voter_store_ids, vec![2]);
    let input = vec![3, 1, 2];
    let s = m
        .update_affinity_group_peers(&ep, &paths, &ss, "g", 2, &input, 100)
        .await
        .unwrap();
    assert_eq!(input, vec![3, 1, 2]);
    assert_eq!(s.state.group.voter_store_ids, vec![1, 2, 3]);
    m.groups.get_mut("g").unwrap().info.affinity_region_count = 7;
    m.groups.get_mut("g").unwrap().info.region_count = 9;
    let s = m
        .update_affinity_group_peers_with_version(
            &ep,
            &paths,
            &ss,
            "g",
            s.affinity_version,
            1,
            &input,
            100,
        )
        .await
        .unwrap();
    assert_eq!(s.state.group.leader_store_id, 1);
    assert_eq!(s.state.affinity_region_count, 0);
    assert_eq!(s.state.region_count, 9);
    for (leader, voters, msg) in [
        (0, vec![1], "must be provided"),
        (1, vec![], "must be provided"),
        (1, vec![1, 1], "duplicate voter"),
        (3, vec![1, 2], "leader must be"),
        (1, vec![1, 9], "store 9 does not exist"),
    ] {
        let err = m
            .update_affinity_group_peers(&ep, &paths, &ss, "g", leader, &voters, 100)
            .await
            .unwrap_err();
        assert!(err.to_string().contains(msg), "{err}");
    }
    assert!(
        m.update_affinity_group_peers(&ep, &paths, &ss, "missing", 1, &[1], 100)
            .await
            .unwrap_err()
            .to_string()
            .contains("ErrAffinityGroupNotFound")
    );
    m.apply_store_availability([(1, crate::policy::StoreCondition::Busy)].into(), 100);
    assert!(
        m.update_affinity_group_peers_with_version(&ep, &paths, &ss, "g", 1, 1, &[1], 100)
            .await
            .unwrap_err()
            .to_string()
            .contains("store 1 is busy")
    );
    // Busy followers are permitted. A successful update resets affinity count/version.
    m.update_affinity_group_peers(&ep, &paths, &ss, "g", 2, &[1, 2], 100)
        .await
        .unwrap();
    let before = m.group_observation("g", 100).unwrap();
    gate.writes.store(1, Ordering::SeqCst);
    assert!(
        m.update_affinity_group_peers(&ep, &paths, &ss, "g", 3, &[3], 100)
            .await
            .is_err()
    );
    assert_eq!(m.group_observation("g", 100).unwrap().state, before.state);
    assert_eq!(m.groups["g"].affinity_version, before.affinity_version);
    let mut persisted = vec![];
    ep.load_all_affinity_groups(&paths, |_, bytes| {
        persisted.push(serde_json::from_slice::<Group>(bytes).unwrap())
    })
    .await
    .unwrap();
    assert_eq!(persisted[0], before.state.group);
    // Observation suppresses I/O errors and preserves state.
    m.expire_affinity_group("g", 100);
    let token = m.group_observation("g", 100).unwrap();
    gate.writes.store(1, Ordering::SeqCst);
    m.observe_available_region(&ep, &paths, &ss, &region(3), Some(&token), 100)
        .await;
    assert_eq!(m.groups["g"].affinity_version, token.affinity_version);
    m.observe_available_region(&ep, &paths, &ss, &region(3), None, 100)
        .await;
}
#[tokio::test]
async fn affinity_update_peers_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn affinity_update_peers_etcd() {
    let f = fixture::Fixture::start();
    contract(Arc::new(f.store().await)).await;
}
