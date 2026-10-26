// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, DownPeer, Peer, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{HotPeerStat, StoreLoadDetail, StoreLoadPred, StoreSummaryInfo};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
#[tokio::test]
async fn task_100_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_100.json"
    ))
    .unwrap();
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        for placement in [false, true] {
            for v2 in [false, true] {
                let mut c = BasicCluster::new();
                for id in 1..=5 {
                    if case["missing_source"] == true && id == 1 {
                        continue;
                    }
                    if case["no_target"] == true && id > 3 {
                        continue;
                    }
                    let mut s = StoreInfo::new(StoreMeta {
                        labels: vec![StoreLabel {
                            key: "zone".into(),
                            value: format!(
                                "z{}",
                                if case["placement_target"] == true && id == 5 {
                                    2
                                } else {
                                    id
                                }
                            ),
                        }],
                        id,
                        ..Default::default()
                    });
                    s.set_last_heartbeat_unix_nanos(100);
                    if case["offline"] == true && id > 3 {
                        s.set_store_state(astersql_pd_core::StoreState::Offline, Some(false));
                    }
                    if case["evicted"] == true && id > 3 {
                        s.slow_trend_evicted();
                    }
                    c.stores.put_store(Arc::new(s));
                }
                let peers: Vec<_> = (1..=3)
                    .map(|id| Peer {
                        id,
                        store_id: id,
                        ..Default::default()
                    })
                    .collect();
                let r = Arc::new(RegionInfo::new(
                    RegionMeta {
                        id: 1,
                        peers: peers.clone(),
                        ..Default::default()
                    },
                    if case["no_leader"] == true {
                        None
                    } else {
                        Some(peers[0].clone())
                    },
                    if case["down"] == true {
                        vec![DownPeer {
                            peer: peers[1].clone(),
                            down_seconds: 1,
                        }]
                    } else {
                        vec![]
                    },
                    if case["pending"] == true {
                        vec![peers[1].clone()]
                    } else {
                        vec![]
                    },
                ));
                if case["missing"] != true {
                    let (o, v, b) = c.regions.set_region(r.clone());
                    c.regions.update_sub_tree(r.clone(), o.as_deref(), &v, b);
                }
                let mut config = ProviderSnapshot::<()>::default();
                config.replication.max_replicas = 3;
                config.replication.location_labels = vec!["zone".into()].into();
                config.replication.enable_placement_rules = placement;
                let mut rules = RuleManager::new(
                    Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
                    ClusterPaths::new(100),
                );
                rules
                    .initialize(3, &["zone".into()], "", false, &config, &[])
                    .await
                    .unwrap();
                let mut hot_peers = vec![HotPeerStat {
                    region_id: 1,
                    store_id: 2,
                    ..Default::default()
                }]; // Move actual leader, not sampled follower.
                if case["bad_peer"] == true {
                    hot_peers.insert(
                        0,
                        HotPeerStat {
                            region_id: 99,
                            ..Default::default()
                        },
                    );
                }
                if case["empty"] == true {
                    hot_peers.clear();
                }
                let detail = StoreLoadDetail {
                    store_summary_info: StoreSummaryInfo {
                        store_info: c.stores.get_store(2).unwrap().as_ref().clone(),
                        ..Default::default()
                    },
                    load_pred: StoreLoadPred::default(),
                    hot_peers,
                };
                let mut resources: [Vec<StoreLoadDetail>; 4] =
                    std::array::from_fn(|_| vec![detail.clone()]);
                // Prove dispatch uses exactly the selected resource snapshot.
                let selected = case["resource"].as_u64().unwrap() as usize;
                for (index, details) in resources.iter_mut().enumerate() {
                    if index != selected {
                        details.clear();
                    }
                }
                let mut allocate = || {
                    if case["allocation"] == true {
                        Err(BuilderError::Allocation("task100 fault".into()))
                    } else {
                        Ok(100)
                    }
                };
                let mut ctx = ShuffleHotContext {
                    cluster: &c,
                    config: &config,
                    rules: &mut rules,
                    now: 100,
                    supports_conf_change_v2: v2,
                    allocate: &mut allocate,
                };
                let s = ShuffleHotScheduler::new(ShuffleHotConfig::default());
                let out = s
                    .schedule(
                        &resources,
                        [
                            selected + 4,
                            case["random_peer"].as_u64().unwrap_or(0) as usize,
                            case["random"].as_u64().unwrap_or(0) as usize,
                        ],
                        &mut ctx,
                    )
                    .await;
                assert_eq!(
                    out.operators.len(),
                    case["expected"].as_u64().unwrap() as usize,
                    "{case} placement={placement} v2={v2}, errors={:?}",
                    out.errors
                );
                if case["allocation"] == true {
                    assert!(matches!(
                        out.errors.as_slice(),
                        [ShuffleHotError::Builder(BuilderError::Allocation(_))]
                    ));
                }
                for op in &out.operators {
                    let dst = case["target"].as_u64().unwrap();
                    assert_eq!(op.region_id(), 1);
                    assert_eq!(op.desc(), "random-move-hot-leader");
                    assert_eq!(op.priority_level(), PriorityLevel::LOW);
                    assert_eq!(op.kind(), OpKind::REGION | OpKind::LEADER);
                    let steps: Vec<_> = (0..op.len()).map(|i| op.step(i).unwrap()).collect();
                    assert!(steps.iter().any(
                        |s| matches!(s,OpStep::AddLearner(a) if a.to_store==dst && a.peer_id==100)
                    ));
                    assert!(
                        steps
                            .iter()
                            .any(|s| matches!(s,OpStep::RemovePeer(a) if a.from_store==1))
                    );
                    assert_eq!(
                        steps
                            .iter()
                            .filter_map(|s| if let OpStep::TransferLeader(t) = s {
                                Some(t.to_store)
                            } else {
                                None
                            })
                            .last(),
                        Some(dst)
                    );
                    assert_eq!(op.conf_ver_changed(&r), 0);
                    let influence = OpInfluence::new();
                    op.total_influence(&influence, Some(&r));
                    assert_eq!(influence.get_store_influence(1).region_count, -1);
                    assert_eq!(influence.get_store_influence(dst).region_count, 1);
                    assert_eq!(influence.get_store_influence(1).leader_count, -1);
                    assert_eq!(influence.get_store_influence(dst).leader_count, 1);
                    let add = steps
                        .iter()
                        .find_map(|s| {
                            if let OpStep::AddLearner(a) = s {
                                Some(a)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    let cmd = add.get_cmd(&r, v2).unwrap();
                    let change = if v2 {
                        cmd.change_peer_v2.unwrap().changes[0].clone().unwrap()
                    } else {
                        cmd.change_peer.unwrap()
                    };
                    assert_eq!(change.peer.unwrap().store_id, dst);
                }
                count += 1;
            }
        }
    }
    println!("task_100: {count} source-derived behavior cases");
}
#[test]
fn task_100_config_and_limits() {
    assert_eq!(ShuffleHotConfig::default().limit, 1);
    for (args, want) in [
        (vec![], 1),
        (vec!["2"], 2),
        (vec!["", ""], 1),
        (vec!["0"], 0),
    ] {
        assert_eq!(ShuffleHotConfig::from_args(&args).unwrap().limit, want);
    }
    for args in [vec![""], vec!["-1"], vec!["18446744073709551616"]] {
        assert!(ShuffleHotConfig::from_args(&args).is_err());
    }
    for (counts, limits, want) in [
        ([0, 0, 0], [1, 1, 1], true),
        ([1, 0, 0], [1, 1, 1], false),
        ([0, 1, 0], [1, 1, 1], false),
        ([0, 0, 1], [1, 1, 1], false),
        ([0, 0, 0], [0, 1, 1], false),
        ([0, 0, 0], [1, 0, 1], false),
        ([0, 0, 0], [1, 1, 0], false),
        ([u64::MAX - 1; 3], [u64::MAX; 3], true),
        ([u64::MAX; 3], [u64::MAX; 3], false),
    ] {
        let scheduler = ShuffleHotScheduler::new(ShuffleHotConfig { limit: limits[0] });
        assert_eq!(
            scheduler.is_schedule_allowed(counts, limits[1], limits[2]),
            want
        );
    }
}
#[tokio::test]
async fn task_100_persistence_faults() {
    let backend = Arc::new(FaultStore::default());
    let endpoint = Arc::new(StorageEndpoint::new(backend.clone()));
    let paths = ClusterPaths::new(100);
    let mut scheduler = ShuffleHotScheduler::new(ShuffleHotConfig::default());
    scheduler.persist(&endpoint, &paths).await.unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(
        scheduler
            .update(ShuffleHotConfig { limit: 8 }, &endpoint, &paths)
            .await
            .is_err()
    );
    assert_eq!(scheduler.config().limit, 1);
    assert!(scheduler.reload(&endpoint, &paths).await.is_err());
    assert_eq!(scheduler.config().limit, 1);
    backend.fail.store(false, Ordering::SeqCst);
    scheduler
        .update(ShuffleHotConfig { limit: 8 }, &endpoint, &paths)
        .await
        .unwrap();
    let mut restored = ShuffleHotScheduler::new(ShuffleHotConfig::default());
    restored.reload(&endpoint, &paths).await.unwrap();
    assert_eq!(restored.config().limit, 8);
    for bytes in [b"bad".as_slice(), b"{\"limit\":\"bad\"}", b"{\"limit\":-1}"] {
        endpoint
            .save_scheduler_config(&paths, "shuffle-hot-region-scheduler", bytes)
            .await
            .unwrap();
        assert!(restored.reload(&endpoint, &paths).await.is_err());
        assert_eq!(restored.config().limit, 8);
    }
    endpoint
        .save_scheduler_config(&paths, "shuffle-hot-region-scheduler", b"{}")
        .await
        .unwrap();
    restored.reload(&endpoint, &paths).await.unwrap();
    assert_eq!(restored.config().limit, 0);
}

use std::sync::atomic::Ordering;
/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
    stall: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl astersql_pd_storage_kv::MetaStore for FaultStore {
    async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<Mutation>,
    ) -> astersql_pd_storage_kv::Result<Commit> {
        if self.stall.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> astersql_pd_storage_kv::Result<Commit> {
        self.store.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> astersql_pd_storage_kv::Result<RawTxnResponse> {
        self.store.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: i64,
    ) -> astersql_pd_storage_kv::Result<Box<dyn MetaWatch>> {
        self.store.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> {
        self.store.grant(t).await
    }
    async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> {
        self.store.keep_alive(l).await
    }
    async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> {
        self.store.revoke(l).await
    }
    async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> {
        self.store.ttl(l).await
    }
    async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> {
        self.store.compact(r).await
    }
}

use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};
