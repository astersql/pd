// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta, StoreStats};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::{Arc, atomic::Ordering};
fn cluster() -> BasicCluster {
    let c = BasicCluster::new();
    for id in 1..=5 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            last_heartbeat: NOW as i64,
            ..Default::default()
        })));
    }
    c
}
fn stopping(c: &BasicCluster, id: u64, value: bool) {
    let mut s = c.stores.get_store(id).unwrap().as_ref().clone();
    s.set_new_store_stats(StoreStats {
        is_stopping: value,
        ..Default::default()
    });
    c.stores.put_store(Arc::new(s));
}
#[tokio::test]
async fn task_082_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_082.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let c = cluster();
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(82);
        for id in case["stopping"].as_array().unwrap() {
            stopping(&c, id.as_u64().unwrap(), true);
        }
        let mut s = EvictStoppingScheduler::new(EvictStoppingConfig::default());
        assert!(s.is_schedule_allowed(10, 0));
        s.observe(&c, &storage, &paths).await.unwrap();
        assert_eq!(s.config().evict_store(), case["evicted"].as_u64().unwrap());
        if s.config().evict_store() != 0 {
            assert!(c.stores.get_store(1).unwrap().evicted_as_stopping_store());
            assert!(!s.is_schedule_allowed(1, 1));
            assert!(s.is_schedule_allowed(0, 1));
            stopping(&c, 1, false);
            s.observe(&c, &storage, &paths).await.unwrap();
            assert_eq!(s.config().evict_store(), 0);
            assert!(!c.stores.get_store(1).unwrap().evicted_as_stopping_store());
        }
    }
    println!("task_082: 3/3 source-derived detection/recovery cases");
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
async fn task_082_persist_reload_prepare_and_faults() {
    let c = cluster();
    stopping(&c, 1, true);
    let paths = ClusterPaths::new(82);
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let mut s = EvictStoppingScheduler::new(EvictStoppingConfig::default());
    backend.fail.store(true, Ordering::SeqCst);
    assert!(s.observe(&c, &storage, &paths).await.is_err());
    assert_eq!(s.config().evict_store(), 0);
    assert!(!c.stores.get_store(1).unwrap().evicted_as_stopping_store());
    backend.fail.store(false, Ordering::SeqCst);
    backend.stall.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            s.observe(&c, &storage, &paths)
        )
        .await
        .is_err()
    );
    assert_eq!(s.config().evict_store(), 0);
    assert!(!c.stores.get_store(1).unwrap().evicted_as_stopping_store());
    backend.stall.store(false, Ordering::SeqCst);
    s.observe(&c, &storage, &paths).await.unwrap();
    let bytes = storage
        .load_scheduler_config(&paths, "evict-stopping-store-scheduler")
        .await
        .unwrap();
    let restored: EvictStoppingConfig = decode_config(&bytes).unwrap();
    assert_eq!(restored.evict_store(), 1);
    let c2 = cluster();
    let mut rebuilt = EvictStoppingScheduler::new(restored);
    rebuilt.prepare(&c2).unwrap();
    assert!(c2.stores.get_store(1).unwrap().evicted_as_stopping_store());
    rebuilt.clean(&c2, &storage, &paths).await.unwrap();
    assert!(!c2.stores.get_store(1).unwrap().evicted_as_stopping_store());
    backend.fail.store(true, Ordering::SeqCst);
    assert!(s.update(5., &storage, &paths).await.is_err());
    assert_eq!(s.config().batch(), 3);
    assert!(s.clean(&c, &storage, &paths).await.is_err());
    assert_eq!(s.config().evict_store(), 1);
    assert!(!c.stores.get_store(1).unwrap().evicted_as_stopping_store());
    backend.fail.store(false, Ordering::SeqCst);
    // Last successful write came from rebuilt Clean. Failed update/cleanup cannot replace it.
    let saved: EvictStoppingConfig = decode_config(
        &storage
            .load_scheduler_config(&paths, "evict-stopping-store-scheduler")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved.evict_store(), 0);
    assert_eq!(saved.batch(), 3);
    s.clean(&c, &storage, &paths).await.unwrap();
    assert_eq!(s.config().evict_store(), 0);
    s.update(5.9, &storage, &paths).await.unwrap();
    assert_eq!(s.config().batch(), 5);
    for n in [0., 11., f64::NAN] {
        assert!(s.update(n, &storage, &paths).await.is_err());
    }
    let cfg: EvictStoppingConfig = decode_config(br#"{"evicted-stores":[99],"batch":0}"#).unwrap();
    s.reload(cfg);
    assert_eq!(s.config().batch(), 3);
    assert!(s.prepare(&c).is_err());
    for bad in [
        b"{".as_slice(),
        br#"{"batch":101}"#,
        br#"{"evicted-stores":[-1]}"#,
    ] {
        assert!(decode_config::<EvictStoppingConfig>(bad).is_err());
    }
}
#[tokio::test]
async fn task_082_lifecycle() {
    use astersql_pd_core::NodeState;
    for state in [None, Some(NodeState::Removed), Some(NodeState::Removing)] {
        let c = cluster();
        stopping(&c, 1, true);
        let paths = ClusterPaths::new(82);
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let mut s = EvictStoppingScheduler::new(EvictStoppingConfig::default());
        s.observe(&c, &storage, &paths).await.unwrap();
        if let Some(state) = state {
            let mut store = c.stores.get_store(1).unwrap().as_ref().clone();
            let mut meta = store.meta().unwrap().clone();
            meta.node_state = state;
            store.set_store_meta(&meta);
            c.stores.put_store(Arc::new(store));
        } else {
            c.stores.delete_store(1);
        }
        s.observe(&c, &storage, &paths).await.unwrap();
        assert_eq!(
            s.config().evict_store(),
            if state == Some(NodeState::Removing) {
                1
            } else {
                0
            }
        );
    }
    // Removing nodes are excluded on first detection; Preparing nodes are eligible.
    for (state, expected) in [
        (NodeState::Removing, 0),
        (NodeState::Removed, 0),
        (NodeState::Preparing, 1),
    ] {
        let c = cluster();
        stopping(&c, 1, true);
        let mut store = c.stores.get_store(1).unwrap().as_ref().clone();
        let mut meta = store.meta().unwrap().clone();
        meta.node_state = state;
        store.set_store_meta(&meta);
        c.stores.put_store(Arc::new(store));
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let mut s = EvictStoppingScheduler::new(EvictStoppingConfig::default());
        s.observe(&c, &storage, &ClusterPaths::new(82))
            .await
            .unwrap();
        assert_eq!(s.config().evict_store(), expected);
    }
}

use astersql_pd_schedule_core::BasicCluster as _;
const NOW: i128 = 1_000_000_000_000;
#[tokio::test]
async fn task_082_real_operators_and_batches() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta, RegionOption};
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    for batch in [1, 3, 5, 10, 100] {
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
        stopping(&c, 1, true);
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
        let mut scheduler = EvictStoppingScheduler::new(EvictStoppingConfig::new(batch).unwrap());
        let mut ctx = EvictLeaderContext {
            cluster: &c,
            config: &config,
            rules: &mut rules,
            now_unix_nanos: NOW,
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        for id in [2, 3] {
            c.stores
                .pause_leader_transfer(id, astersql_pd_core_constant::Direction::IN)
                .unwrap();
        }
        let (blocked, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(blocked.operators.is_empty());
        assert_eq!(scheduler.config().evict_store(), 1);
        for id in [2, 3] {
            c.stores
                .resume_leader_transfer(id, astersql_pd_core_constant::Direction::IN);
        }
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), batch);
        let mut ids = std::collections::HashSet::new();
        for op in result.operators {
            assert!(ids.insert(op.region_id()));
            assert_eq!(op.desc(), "evict-stopping-store-scheduler");
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
        stopping(&c, 1, false);
        scheduler.schedule(&mut ctx, &storage, &paths).await;
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_none());
        assert!(result.operators.is_empty());
    }
    println!("task_082: 5/5 real operator batch cases (1, 3, 5, 10, 100)");
}
