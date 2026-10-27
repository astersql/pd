// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoreStats,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::AffinityFilter;
use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::StatisticsSnapshot;
use astersql_pd_storage_domain::StorageEndpoint;
use astersql_pd_storage_domain::{
    Commit, Compare, Lease, MemoryStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read,
    Snapshot, StoreError,
};
use std::sync::{Arc, atomic::Ordering};
#[tokio::test]
async fn task_099_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_099.json"
    ))
    .unwrap();
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        for placement in [false, true] {
            for v2 in [false, true] {
                let mut cluster = BasicCluster::new();
                for id in 1..=5 {
                    let mut store = StoreInfo::new(StoreMeta {
                        id,
                        ..Default::default()
                    });
                    store.set_last_heartbeat_unix_nanos(1_000_000_000_000);
                    // Global scores deliberately disagree with the range; nonzero used enables projection.
                    store.set_region_size(1000);
                    store.set_region_count(1000);
                    store.set_leader_count(if id == 5 { 1000 } else { 0 });
                    store.update_raw_stats(StoreStats {
                        capacity: 6 * (1 << 40),
                        available: 6 * (1 << 40) - (1000 << 20),
                        used_size: 1000 << 20,
                        ..Default::default()
                    });
                    cluster.stores.put_store(Arc::new(store));
                }
                for id in 1..=if case["empty"] == true { 0 } else { 24 } {
                    let peers: Vec<_> = (1..=3)
                        .map(|s| Peer {
                            id: id * 10 + s,
                            store_id: s,
                            ..Default::default()
                        })
                        .collect();
                    let leader = if case["balanced_leaders"] == true {
                        (id as usize - 1) % 3
                    } else {
                        0
                    };
                    let mut region = RegionInfo::new(
                        RegionMeta {
                            id,
                            key_range: KeyRange {
                                start_key: format!("a{id:03}").into_bytes(),
                                end_key: format!("a{:03}", id + 1).into_bytes(),
                            },
                            peers: peers.clone(),
                            ..Default::default()
                        },
                        Some(peers[leader].clone()),
                        if case["down"] == true {
                            vec![astersql_pd_core::DownPeer {
                                peer: peers[0].clone(),
                                down_seconds: 0,
                            }]
                        } else {
                            vec![]
                        },
                        if case["pending"] == true {
                            vec![peers[0].clone()]
                        } else {
                            vec![]
                        },
                    );
                    region.stats_mut().approximate_size =
                        if case["zero_size"] == true { 0 } else { 1 };
                    let r = Arc::new(region);
                    let (o, v, b) = cluster.regions.set_region(r.clone());
                    cluster.regions.update_sub_tree(r, o.as_deref(), &v, b);
                }
                let mut config = ProviderSnapshot::<()>::default();
                config.schedule.tolerant_size_ratio = 10000.;
                config.schedule.leader_schedule_limit = case["leader"].as_u64().unwrap();
                config.schedule.region_schedule_limit = case["region"].as_u64().unwrap();
                config.schedule.leader_schedule_policy = "count".into();
                config.schedule.region_score_formula_version = "v1".into();
                config.replication.enable_placement_rules = placement;
                config.replication.max_replicas = 3;
                let storage = Arc::new(MemoryStore::default());
                let endpoint = Arc::new(StorageEndpoint::new(storage.clone()));
                let paths = ClusterPaths::new(99);
                let mut rules = RuleManager::new(endpoint.clone(), paths.clone());
                rules
                    .initialize(3, &[], "", false, &config, &[])
                    .await
                    .unwrap();
                let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
                    StorageEndpoint::new(storage),
                    paths,
                    0,
                )
                .await
                .unwrap();
                let key_ranges = astersql_pd_schedule_keyrange::Manager::new();
                // Child custom names must bypass balance scheduler exclusions.
                key_ranges.append(&[astersql_pd_keyutil::KeyRange::new(b"a", b"b")]);
                let influence = OpInfluence::new();
                let before = influence.snapshot();
                let stats = StatisticsSnapshot::default();
                let mut index = |_: usize| 0;
                let mut shuffle = |_: &mut [usize]| {};
                let mut next = 1000;
                let mut allocate = || {
                    next += 1;
                    if case["fail"] == true {
                        Err(BuilderError::Allocation("injected".into()))
                    } else {
                        Ok(next)
                    }
                };
                let mut scheduler = ScatterRangeScheduler::new(
                    ScatterRangeConfig::from_args(&[
                        "test",
                        if case["miss"] == true { "x" } else { "a" },
                        if case["open_end"] == true {
                            ""
                        } else if case["miss"] == true {
                            "z"
                        } else {
                            "b"
                        },
                    ])
                    .unwrap(),
                );
                let result = scheduler
                    .schedule(
                        &mut RegionBalanceContext {
                            cluster: &cluster,
                            config: &config,
                            rules: &mut rules,
                            affinity: AffinityFilter {
                                manager: None,
                                labels: &labels,
                                now: 1000,
                            },
                            statistics: &stats,
                            hot_degree: 1,
                            key_ranges: &key_ranges,
                            influence: &influence,
                            now_unix_nanos: 1_000_000_000_000,
                            random_index: &mut index,
                            shuffle: &mut shuffle,
                            allocate: &mut allocate,
                            supports_conf_change_v2: v2,
                            allow_empty_regions: false,
                        },
                        case["count"].as_u64().unwrap(),
                    )
                    .await;
                let expected = case["expected"].as_str().unwrap();
                assert_eq!(
                    !result.operators.is_empty(),
                    matches!(expected, "leader" | "region"),
                    "{case} placement={placement} errors={:?}",
                    result.errors
                );
                assert_eq!(!result.errors.is_empty(), expected == "error", "{case}");
                assert!(result.plans.is_empty());
                assert_eq!(influence.snapshot(), before);
                assert_eq!(config.schedule.tolerant_size_ratio, 10000.);
                if let Some(op) = result.operators.first() {
                    assert!(op.region_id() <= 24);
                    assert_eq!(op.desc(), format!("scatter-range-{expected}-test"));
                    assert!(op.kind().0 & OpKind::RANGE.0 != 0);
                    if expected == "leader" {
                        assert_eq!(op.kind(), OpKind::LEADER | OpKind::RANGE);
                        let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                            panic!("transfer expected")
                        };
                        assert_eq!(step.from_store, 1);
                        assert!([2, 3].contains(&step.to_store));
                    } else {
                        assert!(op.kind().0 & OpKind::REGION.0 != 0);
                        assert!(op.len() >= 3);
                        let OpStep::AddLearner(add) = op.step(0).unwrap() else {
                            panic!("add learner expected")
                        };
                        assert!([4, 5].contains(&add.to_store));
                        assert!(add.peer_id > 1000);
                        let region = cluster.regions.get_region(op.region_id()).unwrap();
                        let delta = OpInfluence::new();
                        for i in 0..op.len() {
                            op.step(i).unwrap().influence(&delta, &region);
                        }
                        assert_eq!(delta.get_store_influence(add.to_store).region_count, 1);
                        assert_eq!(
                            delta
                                .snapshot()
                                .values()
                                .map(|v| v.region_count)
                                .sum::<i64>(),
                            0
                        );
                        assert_eq!(op.step(0).unwrap().conf_ver_changed(&region), 0);
                    }
                    // Only the first child operator is decorated, as in Go.
                    for other in result.operators.iter().skip(1) {
                        assert_eq!(other.kind().0 & OpKind::RANGE.0, 0);
                        assert_eq!(other.desc(), "scatter-range-leader");
                    }
                }
                count += 1;
            }
        }
    }
    println!("task_099 effective behavior cases: {count}");
}
#[tokio::test]
async fn task_099_config_and_faults() {
    for args in [
        vec![],
        vec!["n"],
        vec!["n", "a"],
        vec!["n", "a", "b", "extra"],
    ] {
        assert!(ScatterRangeConfig::from_args(&args).is_err());
    }
    let cfg = ScatterRangeConfig::from_args(&["", "z", "a"]).unwrap();
    assert_eq!(cfg.start_key, "z");
    let backend = Arc::new(FaultStore::default());
    let endpoint = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(99);
    let mut s =
        ScatterRangeScheduler::new(ScatterRangeConfig::from_args(&["test", "a+%zz", "b"]).unwrap());
    assert_eq!(s.name(), "scatter-range-scheduler-test");
    s.persist(&endpoint, &paths).await.unwrap();
    assert!(
        s.update(Some("rename"), Some("x"), None, &endpoint, &paths)
            .await
            .is_err()
    );
    assert_eq!(s.config().start_key, "a+%zz");
    backend.fail.store(true, Ordering::SeqCst);
    assert!(s.persist(&endpoint, &paths).await.is_err());
    assert!(
        s.update(None, Some("c"), Some("d"), &endpoint, &paths)
            .await
            .is_err()
    );
    assert_eq!(s.config().start_key, "c");
    assert!(s.reload(&endpoint, &paths).await.is_err());
    assert_eq!(s.config().start_key, "c");
    backend.fail.store(false, Ordering::SeqCst);
    s.reload(&endpoint, &paths).await.unwrap();
    assert_eq!(s.config().start_key, "a+%zz");
    s.update(None, Some(""), Some(""), &endpoint, &paths)
        .await
        .unwrap();
    s.reload(&endpoint, &paths).await.unwrap();
    assert_eq!(s.config().end_key, "");
    for bad in [b"{".as_slice(), b"{\"start-key\":1}"] {
        endpoint
            .save_scheduler_config(&paths, s.name(), bad)
            .await
            .unwrap();
        assert!(s.reload(&endpoint, &paths).await.is_err());
        assert_eq!(s.config().start_key, "");
    }
    endpoint
        .save_scheduler_config(
            &paths,
            s.name(),
            br#"{"range-name":"loaded","start-key":"p","end-key":"q"}"#,
        )
        .await
        .unwrap();
    s.reload(&endpoint, &paths).await.unwrap();
    assert_eq!(s.name(), "scatter-range-scheduler-test");
    assert_eq!(s.config().range_name, "loaded");
    for (count, leader, region, want) in [
        (0, 0, 0, false),
        (0, 1, 0, true),
        (0, 0, 1, true),
        (1, 1, 2, true),
        (2, 1, 2, false),
        (u64::MAX, u64::MAX, u64::MAX, false),
    ] {
        assert_eq!(
            ScatterRangeScheduler::is_schedule_allowed(count, leader, region),
            want
        );
    }
    println!("task_099 config/fault contract passed");
}

/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
    pause: std::sync::atomic::AtomicBool,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl astersql_pd_storage_domain::MetaStore for FaultStore {
    async fn read(&self, r: Read) -> astersql_pd_storage_domain::Result<Snapshot> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<Mutation>,
    ) -> astersql_pd_storage_domain::Result<Commit> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        if self.pause.load(std::sync::atomic::Ordering::SeqCst) {
            self.entered.notify_one();
            self.resume.notified().await;
        }
        self.store.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> astersql_pd_storage_domain::Result<Commit> {
        self.store.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> astersql_pd_storage_domain::Result<RawTxnResponse> {
        self.store.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: i64,
    ) -> astersql_pd_storage_domain::Result<Box<dyn MetaWatch>> {
        self.store.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage_domain::Result<Lease> {
        self.store.grant(t).await
    }
    async fn keep_alive(&self, l: i64) -> astersql_pd_storage_domain::Result<Lease> {
        self.store.keep_alive(l).await
    }
    async fn revoke(&self, l: i64) -> astersql_pd_storage_domain::Result<()> {
        self.store.revoke(l).await
    }
    async fn ttl(&self, l: i64) -> astersql_pd_storage_domain::Result<i64> {
        self.store.ttl(l).await
    }
    async fn compact(&self, r: i64) -> astersql_pd_storage_domain::Result<()> {
        self.store.compact(r).await
    }
}
