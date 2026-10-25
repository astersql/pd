// Copyright 2026 AsterSQL.
use crate::BasicCluster;
use astersql_pd_core::{
    BasicCluster as CoreCluster, KeyRange, Peer, RegionEpoch, RegionHeartbeatStats, RegionInfo,
    RegionMeta, StoreControlError, StoreInfo, StoreMeta,
};
use astersql_pd_core_constant::Direction;
use serde_json::{Value, json};
use std::sync::Arc;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_002.json"
    ))
    .unwrap()
}

// Exact first two generateTestRegions(1000, 5) payloads, with the second ID
// and range changed by the recorded Go debugger input; sizes are domain MiB.
fn region(updated: bool) -> Arc<RegionInfo> {
    let i = u64::from(updated);
    let peers: Vec<_> = (0..3)
        .map(|j| Peer {
            id: i * 5 + j + 1,
            store_id: i + j + 1,
            is_witness: !updated && j == 1,
            ..Default::default()
        })
        .collect();
    let mut region = RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: KeyRange {
                start_key: format!("{:20}", 0).into_bytes(),
                end_key: format!("{:20}", 10).into_bytes(),
            },
            epoch: Some(RegionEpoch {
                version: 100 + i,
                conf_ver: 100,
            }),
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    );
    region.set_heartbeat_stats(
        RegionHeartbeatStats {
            approximate_size: 10 * 1048576,
            approximate_keys: 10,
            ..Default::default()
        },
        0,
    );
    Arc::new(region)
}

fn store_ids(stores: Vec<Arc<StoreInfo>>) -> Vec<u64> {
    let mut ids: Vec<_> = stores.iter().map(|s| s.id()).collect();
    ids.sort_unstable();
    ids
}
fn host() -> CoreCluster {
    let cluster = CoreCluster::new();
    for id in 1..=4 {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    cluster
}
fn observe(cluster: &dyn BasicCluster, rejected: Vec<u64>, name: &str) -> Value {
    let current = cluster.get_region(1);
    json!({
        "name": name,
        "version": current.as_ref().map(|r| r.meta().epoch.unwrap().version),
        "leader": current.as_ref().and_then(|r| cluster.get_leader_store(r)).map(|s| s.id()),
        "stores": current.as_ref().map_or_else(Vec::new, |r| store_ids(cluster.get_region_stores(r))),
        "count": cluster.total_region_count(),
        "rejected": rejected,
    })
}

#[test]
fn task_002_behavior_cases() {
    let fixture = fixture();
    let mut cluster = host();
    let original = region(false);
    let replacement = region(true);
    assert_eq!(observe(&cluster, vec![], "empty"), fixture["regions"][0]);
    assert!(BasicCluster::get_store(&cluster, 999).is_none());
    assert!(BasicCluster::get_region(&cluster, 9999).is_none());
    for (index, incoming) in [original.clone(), replacement.clone(), original.clone()]
        .into_iter()
        .enumerate()
    {
        let rejected = cluster
            .regions
            .check_and_put_region(incoming)
            .iter()
            .map(|r| r.meta().id)
            .collect();
        let name = fixture["regions"][index + 1]["name"].as_str().unwrap();
        assert_eq!(
            observe(&cluster, rejected, name),
            fixture["regions"][index + 1]
        );
    }
    assert!(Arc::ptr_eq(
        &BasicCluster::get_region(&cluster, 1).unwrap(),
        &replacement
    ));
    assert_eq!(original.meta().epoch.unwrap().version, 100);
    assert_eq!(BasicCluster::store_region_count(&cluster, 1), 0);
    assert_eq!(BasicCluster::store_region_count(&cluster, 2), 1);
    assert_eq!(BasicCluster::store_region_count(&cluster, 4), 1);
    println!("task_002_behavior_cases: 4 oracle stages passed");
}

#[test]
fn task_002_shared_source_target_views() {
    let mut cluster = host();
    cluster.regions.check_and_put_region(region(false));
    for updated in [false, true] {
        if updated {
            cluster.regions.check_and_put_region(region(true));
        }
        let observer: &dyn BasicCluster = &cluster;
        // Separate source and target consumers see the host's exact objects.
        let source = observer.get_region(1).unwrap();
        let target = observer.get_region(1).unwrap();
        assert!(Arc::ptr_eq(&source, &target));
        assert!(std::ptr::eq(observer.stores(), &cluster.stores));
        assert!(std::ptr::eq(observer.regions(), &cluster.regions));
        assert_eq!(
            store_ids(observer.get_region_stores(&source)),
            if updated {
                vec![2, 3, 4]
            } else {
                vec![1, 2, 3]
            }
        );
        assert_eq!(
            observer.get_leader_store(&target).unwrap().id(),
            if updated { 2 } else { 1 }
        );
        assert_eq!(observer.average_region_size(), 10);
    }
    println!("task_002_shared_source_target_views: 2 update phases passed");
}

#[test]
fn task_002_store_controls_and_errors() {
    let cluster = host();
    let observer: &dyn BasicCluster = &cluster;
    let missing = fixture()["controls"]["missing"].as_u64().unwrap();
    assert_eq!(
        observer.pause_leader_transfer(missing, Direction::IN),
        Err(StoreControlError::StoreNotFound(missing))
    );
    assert!(observer.get_store(missing).is_none());
    let retained = observer.get_store(2).unwrap();
    observer.pause_leader_transfer(2, Direction::IN).unwrap();
    assert!(!observer.get_store(2).unwrap().allow_leader_transfer_in());
    assert!(observer.get_store(2).unwrap().allow_leader_transfer_out());
    assert!(retained.allow_leader_transfer_in());
    observer.resume_leader_transfer(2, Direction::IN);
    assert!(observer.get_store(2).unwrap().allow_leader_transfer_in());
    observer.slow_trend_evicted(2).unwrap();
    assert_eq!(
        observer.slow_trend_evicted(2),
        Err(StoreControlError::SlowTrendEvicted(2))
    );
    assert!(observer.get_store(2).unwrap().is_evicted_as_slow_trend());
    observer.slow_trend_recovered(2);
    assert!(!observer.get_store(2).unwrap().is_evicted_as_slow_trend());
    println!("task_002_store_controls_and_errors: 5 oracle transitions passed");
}

#[test]
fn task_002_membership_queries() {
    let cluster = host();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let region = RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    );
    let observer: &dyn BasicCluster = &cluster;
    let fixture = fixture();
    assert_eq!(
        json!(store_ids(observer.get_region_stores(&region))),
        fixture["membership"]["stores"]
    );
    assert_eq!(
        json!(store_ids(observer.get_follower_stores(&region))),
        fixture["membership"]["followers"]
    );
    assert_eq!(
        json!(observer.get_leader_store(&region).unwrap().id()),
        fixture["membership"]["leader"]
    );
    println!("task_002_membership_queries: 3 oracle queries passed");
}

#[test]
fn task_002_key_lookup_hole_and_boundary() {
    let fixture = fixture();
    let mut cluster = host();
    for input in fixture["key_lookup"]["regions"].as_array().unwrap() {
        let id = input[0].as_u64().unwrap();
        let peer = Peer {
            id,
            store_id: 1,
            ..Default::default()
        };
        cluster
            .regions
            .check_and_put_region(Arc::new(RegionInfo::new(
                RegionMeta {
                    id,
                    key_range: KeyRange {
                        start_key: input[1].as_str().unwrap().as_bytes().to_vec(),
                        end_key: input[2].as_str().unwrap().as_bytes().to_vec(),
                    },
                    peers: vec![peer.clone()],
                    epoch: Some(RegionEpoch {
                        version: 1,
                        conf_ver: 1,
                    }),
                    ..Default::default()
                },
                Some(peer),
                vec![],
                vec![],
            )));
    }
    let observer: &dyn BasicCluster = &cluster;
    for (key, expected) in fixture["key_lookup"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fixture["key_lookup"]["ids"].as_array().unwrap())
    {
        let result = observer.get_region_by_key(key.as_str().unwrap().as_bytes());
        assert_eq!(json!(result.as_ref().map_or(0, |r| r.meta().id)), *expected);
        if let Some(region) = result {
            assert!(Arc::ptr_eq(
                &region,
                &observer.get_region(region.meta().id).unwrap()
            ));
        }
    }
    println!("task_002_key_lookup_hole_and_boundary: 3 oracle queries passed");
}
