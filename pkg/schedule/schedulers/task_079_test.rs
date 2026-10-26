// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster as Cluster, NodeState, StoreInfo, StoreMeta, StoreStats};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::{Arc, atomic::Ordering};
const NOW: i128 = 1_000_000_000_000;
fn cluster() -> Cluster {
    let c = Cluster::new();
    for id in 1..=3 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            last_heartbeat: NOW as i64,
            ..Default::default()
        })));
    }
    c
}
fn score(c: &Cluster, id: u64, value: u64) {
    let mut s = c.get_store(id).unwrap().as_ref().clone();
    s.set_new_store_stats(StoreStats {
        slow_score: value,
        ..Default::default()
    });
    c.stores.put_store(Arc::new(s));
}
#[tokio::test]
async fn task_079_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_079.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let c = cluster();
        for (i, v) in case["scores"].as_array().unwrap().iter().enumerate() {
            score(&c, i as u64 + 1, v.as_u64().unwrap());
        }
        if let Some(state) = case["state"].as_str() {
            let mut s = c.get_store(1).unwrap().as_ref().clone();
            let mut meta = s.meta().unwrap().clone();
            meta.node_state = match state {
                "removed" => NodeState::Removed,
                "removing" => NodeState::Removing,
                _ => NodeState::Serving,
            };
            s.set_store_meta(&meta);
            c.stores.put_store(Arc::new(s));
        }
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let mut s = EvictDiskScheduler::new(EvictDiskConfig::default());
        s.observe(&c, &storage, &ClusterPaths::new(79), NOW)
            .await
            .unwrap();
        assert_eq!(
            s.config().evict_store(),
            case["evict"].as_u64().unwrap(),
            "{case}"
        );
        for id in 1..=3 {
            assert_eq!(
                c.get_store(id).unwrap().evicted_as_slow_store(),
                id == s.config().evict_store(),
                "{case}"
            );
        }
    }
    println!(
        "task_079: {}/{} frozen source-derived detection cases",
        cases.len(),
        cases.len()
    );
}
#[tokio::test]
async fn task_079_recovery_window() {
    let c = cluster();
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(79);
    let mut s = EvictDiskScheduler::new(EvictDiskConfig::new(10, 3).unwrap());
    assert!(s.is_schedule_allowed(100, 0));
    score(&c, 1, 100);
    s.observe(&c, &storage, &paths, NOW).await.unwrap();
    assert!(!s.is_schedule_allowed(3, 3));
    assert!(s.is_schedule_allowed(2, 3));
    score(&c, 1, 1);
    s.observe(&c, &storage, &paths, NOW + 20_000_000_000)
        .await
        .unwrap();
    assert_eq!(s.config().last_capture(), Some(NOW + 20_000_000_000));
    s.observe(&c, &storage, &paths, NOW + 29_999_999_999)
        .await
        .unwrap();
    assert_eq!(s.config().evict_store(), 1);
    score(&c, 1, 2);
    s.observe(&c, &storage, &paths, NOW + 30_000_000_000)
        .await
        .unwrap();
    score(&c, 1, 0);
    s.observe(&c, &storage, &paths, NOW + 40_000_000_000)
        .await
        .unwrap();
    s.observe(&c, &storage, &paths, NOW + 49_999_999_999)
        .await
        .unwrap();
    assert_eq!(s.config().evict_store(), 1);
    s.observe(&c, &storage, &paths, NOW + 50_000_000_000)
        .await
        .unwrap();
    assert_eq!(s.config().evict_store(), 0);
    assert!(!c.get_store(1).unwrap().evicted_as_slow_store());
    assert!(s.config().ready_for_recovery(NOW));
    // isRecovered intentionally survives clearing; recapture preserves Go's subsequent timing.
    score(&c, 1, 100);
    s.observe(&c, &storage, &paths, NOW + 60_000_000_000)
        .await
        .unwrap();
    score(&c, 1, 0);
    s.observe(&c, &storage, &paths, NOW + 70_000_000_000)
        .await
        .unwrap();
    assert_eq!(s.config().evict_store(), 0);
}
#[tokio::test]
async fn task_079_persistence_faults_and_rebuild() {
    let c = cluster();
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(79);
    let mut s = EvictDiskScheduler::new(EvictDiskConfig::new(0, 3).unwrap());
    score(&c, 1, 100);
    backend.fail.store(true, Ordering::SeqCst);
    assert!(s.observe(&c, &storage, &paths, NOW).await.is_err());
    assert_eq!(s.config().evict_store(), 0);
    assert!(!c.get_store(1).unwrap().evicted_as_slow_store());
    backend.fail.store(false, Ordering::SeqCst);
    s.observe(&c, &storage, &paths, NOW).await.unwrap();
    let bytes = storage
        .load_scheduler_config(&paths, "evict-slow-store-scheduler")
        .await
        .unwrap();
    let restored: EvictDiskConfig = decode_config(&bytes).unwrap();
    assert_eq!(restored.last_capture(), None);
    let other = cluster();
    let mut rebuilt = EvictDiskScheduler::new(restored);
    rebuilt.prepare(&other).unwrap();
    assert!(other.get_store(1).unwrap().evicted_as_slow_store());
    rebuilt
        .clean(
            &other,
            &StorageEndpoint::new(Arc::new(MemoryStore::default())),
            &paths,
        )
        .await
        .unwrap();
    assert!(!other.get_store(1).unwrap().evicted_as_slow_store());
    score(&c, 1, 0);
    backend.fail.store(true, Ordering::SeqCst);
    assert!(s.observe(&c, &storage, &paths, NOW + 1).await.is_err());
    assert!(!s.config().is_recovered());
    assert!(c.get_store(1).unwrap().evicted_as_slow_store());
    backend.fail.store(false, Ordering::SeqCst);
    score(&c, 1, 2);
    s.observe(&c, &storage, &paths, NOW + 2).await.unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(s.clean(&c, &storage, &paths).await.is_err());
    assert_eq!(s.config().evict_store(), 1);
    assert!(!c.get_store(1).unwrap().evicted_as_slow_store());
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        storage
            .load_scheduler_config(&paths, "evict-slow-store-scheduler")
            .await
            .unwrap(),
        bytes
    );
    let missing: EvictDiskConfig = decode_config(br#"{"evict-stores":[99]}"#).unwrap();
    assert!(matches!(
        EvictDiskScheduler::new(missing).prepare(&c),
        Err(astersql_pd_core::StoreControlError::StoreNotFound(99))
    ));
    // Cancellation at the I/O boundary must release newly acquired eviction references.
    let c = cluster();
    score(&c, 1, 100);
    let mut s = EvictDiskScheduler::new(EvictDiskConfig::default());
    backend.stall.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            s.observe(&c, &storage, &paths, NOW)
        )
        .await
        .is_err()
    );
    assert_eq!(s.config().evict_store(), 0);
    assert!(!c.get_store(1).unwrap().evicted_as_slow_store());
}
#[test]
fn task_079_config_boundaries() {
    assert_eq!(EvictDiskConfig::default().recovery_sec(), 1800);
    assert_eq!(EvictDiskConfig::default().batch(), 3);
    assert!(
        !EvictDiskConfig::new(u64::MAX, 3)
            .unwrap()
            .ready_for_recovery(NOW)
    );
    for batch in [0, 101, usize::MAX] {
        assert!(EvictDiskConfig::new(10, batch).is_err());
    }
    assert_eq!(
        decode_config::<EvictDiskConfig>(br#"{"batch":0}"#)
            .unwrap()
            .batch(),
        3
    );
    for bad in [
        br#"{"batch":1.5}"#.as_slice(),
        br#"{"batch":101}"#,
        br#"{"evict-stores":[-1]}"#,
        b"{",
    ] {
        assert!(decode_config::<EvictDiskConfig>(bad).is_err());
    }
}

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

#[tokio::test]
async fn task_079_real_operators_and_batches() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta, RegionOption};
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    for batch in [1, 3, 5, 100] {
        let mut c = cluster();
        for id in 0..150 {
            let peers: Vec<_> = (1..=3)
                .map(|s| Peer {
                    id: id * 10 + s,
                    store_id: s,
                    ..Default::default()
                })
                .collect();
            let region = Arc::new(RegionInfo::new(
                RegionMeta {
                    id: id + 1,
                    key_range: KeyRange {
                        start_key: vec![id as u8],
                        end_key: vec![id as u8 + 1],
                    },
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[0].clone()),
                vec![],
                vec![],
            ));
            let (o, v, b) = c.regions.set_region(region.clone());
            c.regions.update_sub_tree(region, o.as_deref(), &v, b);
        }
        score(&c, 1, 100);
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(79);
        let config = ProviderSnapshot::<()>::default();
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            paths.clone(),
        );
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let mut draws = 0;
        let mut random = |bound: usize| {
            let i = draws % bound;
            draws += 1;
            i
        };
        let mut shuffle = |_: &mut [usize]| {};
        let mut scheduler = EvictDiskScheduler::new(EvictDiskConfig::new(0, batch).unwrap());
        let mut ctx = EvictLeaderContext {
            cluster: &c,
            config: &config,
            rules: &mut rules,
            now_unix_nanos: NOW,
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), batch);
        let mut ids = std::collections::HashSet::new();
        for op in result.operators {
            assert!(ids.insert(op.region_id()));
            assert_eq!(op.desc(), "evict-slow-store-scheduler");
            assert_eq!(op.kind(), OpKind::LEADER);
            assert_eq!(op.priority_level(), PriorityLevel::URGENT);
            assert_eq!(op.len(), 1);
            let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                panic!()
            };
            assert_eq!(step.from_store, 1);
            assert_eq!(step.to_stores, vec![2, 3]);
            assert!(step.to_stores.contains(&step.to_store));
            let r = c.get_region(op.region_id()).unwrap();
            assert_eq!(step.conf_ver_changed(&r), 0);
            let influence = OpInfluence::new();
            step.influence(&influence, &r);
            assert_eq!(influence.get_store_influence(1).leader_count, -1);
            assert_eq!(influence.get_store_influence(step.to_store).leader_count, 1);
            for v2 in [false, true] {
                let command = step.get_cmd(&r, v2).transfer_leader.unwrap();
                assert_eq!(command.peer.unwrap().store_id, step.to_store);
                assert_eq!(command.peers.len(), 2);
            }
            assert!(op.start());
            assert!(op.check(&r).is_some());
            let applied = r.clone_with([RegionOption::Leader(
                r.get_store_peer(step.to_store).cloned(),
            )]);
            assert!(op.check(&applied).is_none());
            assert!(op.check_success());
        }
        score(&c, 1, 0);
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(result.operators.is_empty());
    }
    println!("task_079: 4/4 real operator batch cases (1, 3, 5, 100)");
}

#[tokio::test]
async fn task_079_removed_and_missing_recovery() {
    use astersql_pd_core_constant::Direction;
    for removed in [false, true] {
        let mut c = cluster();
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(79);
        let mut s = EvictDiskScheduler::new(EvictDiskConfig::default());
        score(&c, 1, 100);
        s.observe(&c, &storage, &paths, NOW).await.unwrap();
        if removed {
            let mut store = c.get_store(1).unwrap().as_ref().clone();
            let mut meta = store.meta().unwrap().clone();
            meta.node_state = NodeState::Removed;
            store.set_store_meta(&meta);
            c.stores.put_store(Arc::new(store));
        } else {
            c.stores = Default::default();
        }
        s.observe(&c, &storage, &paths, NOW + 1).await.unwrap();
        assert_eq!(s.config().evict_store(), 0);
    }
    let c = cluster();
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(79);
    let mut s = EvictDiskScheduler::new(EvictDiskConfig::default());
    // A second owner, such as network eviction, retains its own reference and pause.
    c.slow_store_evicted(1).unwrap();
    c.pause_leader_transfer(1, Direction::IN).unwrap();
    score(&c, 1, 100);
    s.observe(&c, &storage, &paths, NOW).await.unwrap();
    s.clean(&c, &storage, &paths).await.unwrap();
    assert!(c.get_store(1).unwrap().evicted_as_slow_store());
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_in());
    c.slow_store_recovered(1);
    c.resume_leader_transfer(1, Direction::IN);
    assert!(!c.get_store(1).unwrap().evicted_as_slow_store());
}
