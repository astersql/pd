// Copyright 2026 AsterSQL.
use crate::policy::{StoreCondition, collect_unavailable_stores};
use astersql_pd_core::{NodeState, StoreInfo, StoreMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
#[test]
fn affinity_availability_store_collection() {
    let conf = ProviderSnapshot::<String>::default();
    let s = StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Removed,
        ..Default::default()
    });
    assert_eq!(
        collect_unavailable_stores(&[s], &conf, 0).get(&1),
        Some(&StoreCondition::Removed)
    );
}

use crate::{
    group::{Availability, Group, RuntimeGroupInfo},
    manager::{Manager, RestoredGroup},
    policy::calc_group_availability,
};
use astersql_pd_core::{StoreLabel, StoreStats};
use std::collections::BTreeMap;
fn store(id: u64) -> StoreInfo {
    StoreInfo::new(StoreMeta {
        id,
        node_state: NodeState::Serving,
        ..Default::default()
    })
}
fn condition(s: StoreInfo, nanos: i128) -> Option<StoreCondition> {
    collect_unavailable_stores(&[s], &ProviderSnapshot::<String>::default(), nanos)
        .get(&1)
        .copied()
}
#[test]
fn affinity_availability_priority_and_boundaries() {
    use StoreCondition::*;
    assert_eq!(condition(store(1), 20_000_000_000), None);
    assert_eq!(condition(store(1), 20_000_000_001), Some(Disconnected));
    assert_eq!(condition(store(1), 600_000_000_000), Some(Disconnected));
    assert_eq!(condition(store(1), 600_000_000_001), Some(Down));
    let mut s = store(1);
    s.edit_meta(|m| m.node_state = NodeState::Preparing);
    s.set_new_store_stats(StoreStats {
        capacity: 100,
        available: 0,
        is_busy: true,
        ..Default::default()
    });
    assert_eq!(condition(s.shallow_clone(), 120_000_000_000), Some(Busy));
    s.slow_store_evicted();
    assert_eq!(
        condition(s.shallow_clone(), 120_000_000_000),
        Some(LeaderEvicted)
    );
    assert_eq!(condition(s.shallow_clone(), 601_000_000_000), Some(Down));
    s.edit_meta(|m| m.node_state = NodeState::Removing);
    assert_eq!(
        condition(s.shallow_clone(), 601_000_000_000),
        Some(Removing)
    );
    s.edit_meta(|m| m.physically_destroyed = true);
    assert_eq!(condition(s, 601_000_000_000), Some(Removed));
    let mut s = store(1);
    s.edit_meta(|m| m.node_state = NodeState::Preparing);
    s.set_new_store_stats(StoreStats {
        capacity: 100,
        available: 0,
        ..Default::default()
    });
    assert_eq!(condition(s, 0), Some(LowSpace));
    let mut s = store(1);
    s.set_new_store_stats(StoreStats {
        slow_score: 100,
        ..Default::default()
    });
    assert_eq!(condition(s, 0), None);
    for c in [LeaderEvicted, Busy] {
        assert_eq!(
            calc_group_availability(&BTreeMap::from([(2, c)]), 1, &[1, 2]),
            Availability::Available
        );
        assert_eq!(
            calc_group_availability(&BTreeMap::from([(1, c)]), 1, &[1, 2]),
            Availability::Degraded
        );
    }
    assert_eq!(
        calc_group_availability(
            &BTreeMap::from([(1, Disconnected), (2, Removed)]),
            1,
            &[1, 2]
        ),
        Availability::Expired
    );
    assert_eq!(
        calc_group_availability(&BTreeMap::from([(3, Removed)]), 1, &[1, 2]),
        Availability::Available
    );
}
#[test]
fn affinity_availability_eviction_sources() {
    let mut conf = ProviderSnapshot::<String>::default();
    conf.label_properties.insert(
        "reject-leader".into(),
        vec![StoreLabel {
            key: "reject".into(),
            value: "leader".into(),
        }],
    );
    for i in 0..5 {
        let mut s = store(1);
        match i {
            0 => s.pause_leader_transfer(astersql_pd_core_constant::Direction::IN),
            1 => s.set_labels(vec![StoreLabel {
                key: "reject".into(),
                value: "leader".into(),
            }]),
            2 => s.slow_store_evicted(),
            3 => s.stopping_store_evicted(),
            _ => s.slow_trend_evicted(),
        }
        assert_eq!(
            collect_unavailable_stores(&[s], &conf, 0)[&1],
            StoreCondition::LeaderEvicted
        );
    }
}
#[test]
fn affinity_availability_transitions() {
    let mut m = Manager::default();
    m.groups.insert(
        "g".into(),
        RestoredGroup {
            info: {
                let mut info = RuntimeGroupInfo::default();
                info.group = Group {
                    id: "g".into(),
                    leader_store_id: 1,
                    voter_store_ids: vec![1, 2],
                    ..Default::default()
                };
                info
            },
            affinity_version: 1,
            label_rule: None,
        },
    );
    let degraded = BTreeMap::from([(2, StoreCondition::Disconnected)]);
    assert!(m.apply_store_availability(degraded.clone(), 10));
    let state = m.groups["g"].info.state(610);
    assert!(!state.affinity_scheduling_allowed && !state.regular_scheduling_allowed);
    assert!(!m.apply_store_availability(degraded.clone(), 610));
    assert_eq!(m.groups["g"].info.availability(610), Availability::Degraded);
    assert_eq!(m.groups["g"].info.availability(611), Availability::Expired);
    assert!(m.apply_store_availability(BTreeMap::new(), 611));
    assert!(m.groups["g"].info.state(611).regular_scheduling_allowed);
    m.restore_affinity_group("g", 612);
    assert!(m.groups["g"].info.state(612).affinity_scheduling_allowed);
    m.groups.get_mut("g").unwrap().info.affinity_region_count = 5;
    let version = m.groups["g"].affinity_version;
    m.degrade_affinity_group("g", 620);
    assert_eq!(m.groups["g"].info.affinity_region_count, 0);
    assert_eq!(m.groups["g"].affinity_version, version + 1);
    assert!(m.apply_store_availability(degraded, 621));
    assert!(m.apply_store_availability(BTreeMap::new(), 622));
    assert!(m.groups["g"].info.state(622).affinity_scheduling_allowed);
    m.expire_affinity_group("g", 623);
    m.apply_store_availability(
        BTreeMap::from([(2, StoreCondition::Busy), (1, StoreCondition::Removed)]),
        624,
    );
    assert!(m.check_has_unavailable_store(3, &[2]).is_ok());
    let error = m
        .check_has_unavailable_store(2, &[2, 1])
        .unwrap_err()
        .to_string();
    assert!(error.contains("store 2 is busy"));
    m.apply_store_availability(BTreeMap::new(), 625);
    assert_eq!(m.groups["g"].info.availability(625), Availability::Expired);
    m.restore_affinity_group("missing", 625);
}

#[tokio::test]
async fn affinity_availability_memory_manager() {
    use crate::ranges::GroupKeyRanges;
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_labeler::RegionLabeler;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    let memory = std::sync::Arc::new(MemoryStore::default());
    let ep = StorageEndpoint::new(memory.clone());
    let paths = ClusterPaths::new(36);
    let mut labeler = RegionLabeler::load(StorageEndpoint::new(memory), paths.clone(), 0)
        .await
        .unwrap();
    let mut m = Manager::default();
    m.create_affinity_groups(
        &ep,
        &paths,
        &mut labeler,
        &[GroupKeyRanges {
            group_id: "g".into(),
            key_ranges: vec![],
        }],
        0,
    )
    .await
    .unwrap();
    let g = &mut m.groups.get_mut("g").unwrap().info.group;
    g.leader_store_id = 1;
    g.voter_store_ids = vec![1, 2];
    let conf = ProviderSnapshot::<String>::default();
    let mut s = store(2);
    assert!(m.check_groups_availability(&[s.shallow_clone()], &conf, 120_000_000_000, 120));
    assert_eq!(m.groups["g"].info.availability(120), Availability::Degraded);
    s.set_last_heartbeat_unix_nanos(121_000_000_000);
    m.check_groups_availability(&[s.shallow_clone()], &conf, 121_000_000_000, 121);
    assert_eq!(
        m.groups["g"].info.availability(121),
        Availability::Available
    );
    m.check_groups_availability(&[s.shallow_clone()], &conf, 800_000_000_000, 800);
    assert_eq!(m.groups["g"].info.availability(800), Availability::Expired);
    s.set_last_heartbeat_unix_nanos(801_000_000_000);
    m.check_groups_availability(&[s], &conf, 801_000_000_000, 801);
    assert_eq!(m.groups["g"].info.availability(801), Availability::Expired);
    // Availability is runtime-only: persisted group remains loadable, degraded at startup.
    let restored = Manager::initialize(&ep, &paths, &labeler, 802)
        .await
        .unwrap();
    assert_eq!(
        restored.groups["g"].info.availability(802),
        Availability::Degraded
    );
}
