// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    DownPeer, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreLimiter, StoreMeta,
    StoreStats,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_plan::{BalanceSchedulerPlan, Collector, Status, StatusCode};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use astersql_pd_storelimit::{SlidingWindows, Type};
use serde_json::Value;
use std::{cell::RefCell, sync::Arc};
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_010.json"
    ))
    .unwrap()
}
fn region(n: u64, learner: bool, pending: bool, down: bool) -> Arc<RegionInfo> {
    let peers: Vec<_> = (1..=n)
        .map(|id| Peer {
            id,
            store_id: id,
            role: if learner && id == 1 {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            ..Default::default()
        })
        .collect();
    Arc::new(RegionInfo::new(
        RegionMeta {
            peers: peers.clone(),
            ..Default::default()
        },
        peers.first().cloned(),
        if down {
            vec![DownPeer {
                peer: peers[0].clone(),
                down_seconds: 0,
            }]
        } else {
            vec![]
        },
        if pending {
            vec![peers[0].clone()]
        } else {
            vec![]
        },
    ))
}
async fn manager(conf: &ProviderSnapshot<()>, stores: &[Arc<StoreInfo>]) -> RuleManager {
    let mut m = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(10),
    );
    m.initialize(
        3,
        &[],
        "",
        false,
        conf,
        &stores.iter().map(|s| (**s).clone()).collect::<Vec<_>>(),
    )
    .await
    .unwrap();
    m
}
fn check(status: Status, expected: &Value) {
    assert_eq!(status.status_code.0, expected["code"].as_i64().unwrap());
    assert_eq!(status.detailed_reason, expected["detail"].as_str().unwrap());
}
#[tokio::test]
async fn task_010_behavior_cases() {
    let f = fixture();
    let stores: Vec<_> = (1..=4)
        .map(|id| {
            Arc::new(StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            }))
        })
        .collect();
    let mut count = 0;
    for c in f["cases"].as_array().unwrap() {
        let route = c["route"].as_str().unwrap();
        if route == "health" || route.starts_with("replication") {
            let r = region(
                c["peers"].as_u64().unwrap(),
                c["learner"].as_bool().unwrap(),
                c["pending"].as_bool().unwrap(),
                c["down"].as_bool().unwrap(),
            );
            if route == "health" {
                assert_eq!(
                    is_region_healthy(&r),
                    c["bools"][0].as_bool().unwrap(),
                    "{}",
                    c["id"]
                );
                assert_eq!(
                    is_region_healthy_allow_pending(&r),
                    c["bools"][1].as_bool().unwrap()
                );
                check(RegionPendingFilter.select(&r), &c["statuses"][0]);
                check(RegionDownFilter.select(&r), &c["statuses"][1]);
            } else {
                let mut conf = ProviderSnapshot::<()>::default();
                conf.replication.max_replicas = 3;
                conf.replication.enable_placement_rules = c["rules"].as_bool().unwrap();
                let mut m = manager(&conf, &stores).await;
                let context = RegionReplicationContext {
                    conf: &conf,
                    stores: &stores,
                    manager: RefCell::new(&mut m),
                    now: 0,
                };
                assert_eq!(
                    is_region_replicated(&context, &r),
                    c["bools"][0].as_bool().unwrap()
                );
                let filter = RegionReplicatedFilter::new(context);
                check(filter.select(&r), &c["statuses"][0]);
                assert_eq!(
                    filter.get_fit().is_some(),
                    c["rules"] == true && c["bools"][0] == true
                );
            }
        } else if route.starts_with("empty_") {
            let mut r = (*region(3, false, false, false)).clone();
            r.stats_mut().approximate_size = c["size"].as_i64().unwrap();
            r.stats_mut().approximate_keys = 2;
            check(
                RegionEmptyFilter {
                    total_region_count: c["total"].as_i64().unwrap(),
                }
                .select(&Arc::new(r)),
                &c["statuses"][0],
            );
        } else if route == "witness" {
            let r = Arc::new(
                region(3, false, false, false)
                    .clone_with([astersql_pd_core::RegionOption::Witnesses(vec![2])]),
            );
            check(
                RegionWitnessFilter {
                    store_id: c["store"].as_u64().unwrap(),
                }
                .select(&r),
                &c["statuses"][0],
            );
        } else if route.starts_with("snapshot_") {
            let mut store = StoreInfo::new(StoreMeta {
                id: 1,
                ..Default::default()
            });
            if c["busy"] == true {
                store.set_new_store_stats(StoreStats {
                    is_busy: true,
                    ..Default::default()
                });
            }
            store.set_store_limit(Some(StoreLimiter::V2(SlidingWindows::new())));
            if c["limited"] == true {
                for _ in 0..2 {
                    assert!(
                        store
                            .limiter()
                            .unwrap()
                            .write()
                            .unwrap()
                            .take(1000, Type::SEND_SNAPSHOT, PriorityLevel::MEDIUM)
                            .await
                    );
                }
            }
            let filter = SnapshotSenderFilter::new(&[Arc::new(store)], PriorityLevel::MEDIUM).await;
            let mut r = region(1, false, false, false);
            if route == "snapshot_no_leader" {
                r = Arc::new(r.clone_with([astersql_pd_core::RegionOption::Leader(None)]));
            }
            check(filter.select(&r), &c["statuses"][0]);
        } else if route == "affinity" {
            continue;
        } else {
            panic!("unknown route {route}")
        }
        count += 1;
    }
    assert_eq!(count, 43);
    println!("task_010_behavior_cases: {count} frozen Go cases passed");
}
#[tokio::test]
async fn task_010_fit_retains_last_success_and_selection_order() {
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.max_replicas = 3;
    conf.replication.enable_placement_rules = true;
    let stores: Vec<_> = (1..=3)
        .map(|id| {
            Arc::new(StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            }))
        })
        .collect();
    let mut m = manager(&conf, &stores).await;
    let filter = RegionReplicatedFilter::new(RegionReplicationContext {
        conf: &conf,
        stores: &stores,
        manager: RefCell::new(&mut m),
        now: 0,
    });
    assert!(filter.get_fit().is_none());
    let good = region(3, false, false, false);
    assert!(filter.select(&good).is_ok());
    let fit = filter.get_fit().unwrap();
    assert!(!filter.select(&region(2, false, false, false)).is_ok());
    assert!(Arc::ptr_eq(&fit, &filter.get_fit().unwrap()));
    let bad = region(3, false, true, true);
    let down = region(3, false, false, true);
    let good2 = Arc::new((*good).clone());
    let regions = vec![bad.clone(), down.clone(), good.clone(), good2.clone()];
    let selected = select_regions(&regions, &[&RegionPendingFilter, &RegionDownFilter]);
    assert_eq!(selected.len(), 2);
    assert!(Arc::ptr_eq(&selected[0], &good));
    let mut collector = Collector::new(BalanceSchedulerPlan {
        step: 1,
        ..Default::default()
    });
    let one = select_one_region(
        &regions,
        Some(&mut collector),
        &[&RegionPendingFilter, &RegionDownFilter],
    )
    .unwrap();
    assert!(Arc::ptr_eq(&one, &good));
    let plans = collector.get_plans();
    assert_eq!(plans.len(), 2);
    assert!(
        plans
            .iter()
            .all(|p| p.status.status_code == StatusCode::REGION_UNHEALTHY)
    );
    assert!(select_one_region(&[], None, &[]).is_none());
    assert!(select_regions(&[], &[]).is_empty());
    assert_eq!(select_regions(&regions, &[]).len(), 4);
}
#[tokio::test]
async fn task_010_snapshot_busy_missing_leader_and_snapshot_capture() {
    let r = region(1, false, false, false);
    let mut store = StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    });
    store.set_store_limit(Some(StoreLimiter::V2(SlidingWindows::new())));
    let filter = SnapshotSenderFilter::new(&[Arc::new(store.clone())], PriorityLevel::MEDIUM).await;
    assert!(filter.select(&r).is_ok());
    for _ in 0..2 {
        assert!(
            store
                .limiter()
                .unwrap()
                .write()
                .unwrap()
                .take(1000, Type::SEND_SNAPSHOT, PriorityLevel::MEDIUM)
                .await
        );
    }
    assert!(filter.select(&r).is_ok());
    assert!(
        !SnapshotSenderFilter::new(&[Arc::new(store)], PriorityLevel::MEDIUM)
            .await
            .select(&r)
            .is_ok()
    );
    let mut busy = StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    });
    busy.set_new_store_stats(StoreStats {
        is_busy: true,
        ..Default::default()
    });
    assert!(
        !SnapshotSenderFilter::new(&[Arc::new(busy)], PriorityLevel::MEDIUM)
            .await
            .select(&r)
            .is_ok()
    );
    assert!(
        !SnapshotSenderFilter::new(&[], PriorityLevel::MEDIUM)
            .await
            .select(&r)
            .is_ok()
    );
    let no_leader = Arc::new(r.clone_with([astersql_pd_core::RegionOption::Leader(None)]));
    assert!(!filter.select(&no_leader).is_ok());
}
#[tokio::test]
async fn task_010_affinity_real_manager() {
    use astersql_pd_schedule_affinity::{
        group::Group,
        manager::Manager,
        ranges::{GroupKeyRanges, make_label_rule},
    };
    use astersql_pd_schedule_labeler::RegionLabeler;
    let paths = astersql_pd_keypath::ClusterPaths::new(10);
    let ep = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut labels = RegionLabeler::load(ep, paths, 0).await.unwrap();
    let ranges = GroupKeyRanges {
        group_id: "g1".into(),
        key_ranges: vec![astersql_pd_keyutil::KeyRange::new(
            b"a".to_vec(),
            b"z".to_vec(),
        )],
    };
    let mut rule = make_label_rule(&ranges);
    rule.check_and_adjust(0).unwrap();
    labels.set_label_rule(rule.clone(), 0).await.unwrap();
    let mut manager = Manager::default();
    manager.sync_group_from_etcd(
        &Group {
            id: "g1".into(),
            leader_store_id: 1,
            voter_store_ids: vec![1, 2, 3],
            ..Default::default()
        },
        &labels,
        0,
    );
    manager.sync_key_ranges_from_etcd(&rule).unwrap();
    manager.restore_affinity_group("g1", 0);
    let mut count = 0;
    for c in fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["route"] == "affinity")
    {
        let r = Arc::new(region(3, false, false, false).clone_with([
            astersql_pd_core::RegionOption::StartKey(
                c["start"].as_str().unwrap().as_bytes().to_vec(),
            ),
            astersql_pd_core::RegionOption::EndKey(c["end"].as_str().unwrap().as_bytes().to_vec()),
        ]));
        check(
            AffinityFilter {
                manager: Some(&manager),
                labels: &labels,
                now: 0,
            }
            .select(&r),
            &c["statuses"][0],
        );
        assert!(
            AffinityFilter {
                manager: None,
                labels: &labels,
                now: 0
            }
            .select(&r)
            .is_ok()
        );
        count += 1;
    }
    assert_eq!(count, 2);
    println!("task_010_affinity_real_manager: {count} frozen Go cases passed");
}
