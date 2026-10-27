// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta, StoreStats};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
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
fn score(c: &BasicCluster, id: u64, scores: &[(u64, u64)]) {
    let mut s = c.stores.get_store(id).unwrap().as_ref().clone();
    s.set_new_store_stats(StoreStats {
        network_slow_scores: scores.iter().copied().collect(),
        ..Default::default()
    });
    c.stores.put_store(Arc::new(s));
}
#[tokio::test]
async fn task_081_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_081.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let c = cluster();
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(81);
        let mut s = EvictNetworkScheduler::new(EvictNetworkConfig::new(true, 10, 3).unwrap());
        score(&c, 1, &[(2, 95), (3, 10), (4, 10)]);
        assert!(s.observe(&c, &storage, &paths, 0).await.is_empty());
        for _ in 0..case["triggers"].as_u64().unwrap() {
            c.stores.trigger_network_slow_evict(1);
        }
        assert!(s.observe(&c, &storage, &paths, 1).await.is_empty());
        assert_eq!(
            s.config().evicted.contains(&1),
            case["evicted"].as_bool().unwrap()
        );
        assert_eq!(
            c.stores.get_store(1).unwrap().evicted_as_slow_store(),
            case["evicted"].as_bool().unwrap()
        );
        assert_eq!(s.config().paused, vec![1]);
        score(&c, 1, &[(2, 1), (3, 1), (4, 1)]);
        s.observe(&c, &storage, &paths, 1_000_000_000).await;
        assert_eq!(s.recovering().get(&1), Some(&Some(1_000_000_000)));
        s.observe(&c, &storage, &paths, 10_999_999_999).await;
        assert_eq!(s.config().paused, vec![1]);
        s.observe(&c, &storage, &paths, 11_000_000_000).await;
        assert!(s.config().paused.is_empty());
        assert!(s.config().evicted.is_empty());
        assert!(s.recovering().is_empty());
        assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_in());
        assert!(!c.stores.get_store(1).unwrap().evicted_as_slow_store());
    }
    println!("task_081: 4/4 source-derived trigger/recovery cases");
}

#[tokio::test]
async fn task_081_waiting_activation_and_reset() {
    let c = cluster();
    let paths = ClusterPaths::new(81);
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut s = EvictNetworkScheduler::new(EvictNetworkConfig::new(true, 10, 3).unwrap());
    score(&c, 1, &[(2, 95), (3, 10), (4, 10)]);
    s.observe(&c, &storage, &paths, 0).await;
    score(&c, 2, &[(1, 100), (3, 95), (4, 10), (5, 10)]);
    s.observe(&c, &storage, &paths, 1).await;
    assert!(s.recovering().contains_key(&2));
    assert!(!s.config().paused.contains(&2));
    assert!(c.stores.get_store(2).unwrap().allow_leader_transfer_in());
    // Filter the slow tracked peer's 100; remaining zero scores start recovery.
    score(&c, 1, &[(2, 100), (3, 0), (4, 0)]);
    s.observe(&c, &storage, &paths, 1_000_000_000).await;
    assert_eq!(s.recovering()[&1], Some(1_000_000_000));
    score(&c, 1, &[(3, 2)]);
    s.observe(&c, &storage, &paths, 5_000_000_000).await;
    assert_eq!(s.recovering()[&1], None);
    score(&c, 1, &[]);
    s.observe(&c, &storage, &paths, 6_000_000_000).await;
    s.observe(&c, &storage, &paths, 16_000_000_000).await;
    assert_eq!(s.config().paused, vec![2]);
    assert_eq!(s.recovering()[&2], None);
    assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(!c.stores.get_store(2).unwrap().allow_leader_transfer_in());
    for _ in 0..6 {
        c.stores.trigger_network_slow_evict(2);
    }
    s.observe(&c, &storage, &paths, 17_000_000_000).await;
    assert_eq!(s.config().evicted, vec![2]);
    // Retain another owner's disk reference and pause through network cleanup.
    c.stores.slow_store_evicted(2).unwrap();
    c.stores
        .pause_leader_transfer(2, astersql_pd_core_constant::Direction::IN)
        .unwrap();
    s.update(false, 20, 5, &storage, &paths).await.unwrap();
    s.observe(&c, &storage, &paths, 18_000_000_000).await;
    assert!(s.config().paused.is_empty());
    assert!(s.recovering().is_empty());
    assert!(c.stores.get_store(2).unwrap().evicted_as_slow_store());
    assert!(!c.stores.get_store(2).unwrap().allow_leader_transfer_in());
    c.stores.slow_store_recovered(2);
    c.stores
        .resume_leader_transfer(2, astersql_pd_core_constant::Direction::IN);
    assert!(c.stores.get_store(2).unwrap().allow_leader_transfer_in());
}

#[tokio::test]
async fn task_081_persist_reload_prepare_clean() {
    let c = cluster();
    let paths = ClusterPaths::new(81);
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut s = EvictNetworkScheduler::new(EvictNetworkConfig::new(true, 0, 3).unwrap());
    score(&c, 1, &[(2, 95), (3, 10), (4, 10)]);
    s.observe(&c, &storage, &paths, 0).await;
    for _ in 0..6 {
        c.stores.trigger_network_slow_evict(1);
    }
    s.observe(&c, &storage, &paths, 1).await;
    let bytes = storage
        .load_scheduler_config(&paths, "evict-slow-store-scheduler")
        .await
        .unwrap();
    let restored: EvictNetworkConfig = decode_config(&bytes).unwrap();
    assert_eq!(restored.paused, vec![1]);
    assert_eq!(restored.evicted, vec![1]);
    assert_eq!(restored.disk.batch(), 3);
    let c2 = cluster();
    let mut rebuilt = EvictNetworkScheduler::new(restored);
    assert!(rebuilt.recovering().is_empty());
    assert!(rebuilt.prepare(&c2).is_empty());
    assert_eq!(rebuilt.recovering()[&1], None);
    assert!(!c2.stores.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(c2.stores.get_store(1).unwrap().evicted_as_slow_store());
    assert!(rebuilt.clean(&c2, &storage, &paths).await.is_empty());
    assert!(c2.stores.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(!c2.stores.get_store(1).unwrap().evicted_as_slow_store());
    let next: EvictNetworkConfig = decode_config(
        br#"{"enable-network-slow-store":true,"paused-network-slow-stores":[2],"batch":0}"#,
    )
    .unwrap();
    assert!(rebuilt.reload(next, &c2).is_empty());
    assert!(!c2.stores.get_store(2).unwrap().allow_leader_transfer_in());
    assert!(rebuilt.recovering().is_empty()); // Reload intentionally preserves the runtime map.
    let missing: EvictNetworkConfig =
        decode_config(br#"{"paused-network-slow-stores":[98],"evicted-network-slow-stores":[99]}"#)
            .unwrap();
    let errors = EvictNetworkScheduler::new(missing).prepare(&c2);
    assert_eq!(errors.len(), 2);
    for bad in [
        b"{".as_slice(),
        br#"{"batch":101}"#,
        br#"{"paused-network-slow-stores":[-1]}"#,
    ] {
        assert!(decode_config::<EvictNetworkConfig>(bad).is_err());
    }
    assert!(s.update(true, 0, 101, &storage, &paths).await.is_err());
}

#[tokio::test]
async fn task_081_removed_and_average_boundaries() {
    use astersql_pd_core::NodeState;
    for state in [None, Some(NodeState::Removed), Some(NodeState::Removing)] {
        let c = cluster();
        let paths = ClusterPaths::new(81);
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let mut s = EvictNetworkScheduler::new(EvictNetworkConfig::new(true, 10, 3).unwrap());
        score(&c, 1, &[(2, 95), (3, 10), (4, 10)]);
        s.observe(&c, &storage, &paths, 0).await;
        if let Some(state) = state {
            let mut store = c.stores.get_store(1).unwrap().as_ref().clone();
            let mut meta = store.meta().unwrap().clone();
            meta.node_state = state;
            store.set_store_meta(&meta);
            c.stores.put_store(Arc::new(store));
        } else {
            c.stores.delete_store(1);
        }
        assert!(s.observe(&c, &storage, &paths, 1).await.is_empty());
        if state == Some(NodeState::Removing) {
            // Go includes Removing in detection, so frozen slow scores recapture it this round.
            assert_eq!(s.config().paused, vec![1]);
            assert_eq!(s.recovering()[&1], None);
        } else {
            assert!(s.config().paused.is_empty());
            assert!(s.recovering().is_empty());
        }
    }
    let c = cluster();
    let paths = ClusterPaths::new(81);
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut s = EvictNetworkScheduler::new(EvictNetworkConfig::new(true, 10, 3).unwrap());
    score(&c, 1, &[(2, 95), (3, 10), (4, 10)]);
    s.observe(&c, &storage, &paths, 0).await;
    // Go's uint64 sum wraps, and integer average truncates.
    score(&c, 1, &[(2, u64::MAX), (3, 3)]);
    s.observe(&c, &storage, &paths, 1).await;
    assert_eq!(s.recovering()[&1], Some(1));
    score(&c, 1, &[(2, 2), (3, 1)]);
    s.observe(&c, &storage, &paths, 2).await;
    assert_eq!(s.recovering()[&1], Some(1));
    score(&c, 1, &[(2, 2), (3, 2)]);
    s.observe(&c, &storage, &paths, 3).await;
    assert_eq!(s.recovering()[&1], None);
}

/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
    stall: std::sync::atomic::AtomicBool,
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

use astersql_pd_storage_domain::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};

#[tokio::test]
async fn task_081_io_faults_and_cancellation() {
    let c = cluster();
    let paths = ClusterPaths::new(81);
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let mut s = EvictNetworkScheduler::new(EvictNetworkConfig::new(true, 0, 3).unwrap());
    score(&c, 1, &[(2, 95), (3, 10), (4, 10)]);
    backend.fail.store(true, Ordering::SeqCst);
    assert_eq!(s.observe(&c, &storage, &paths, 0).await.len(), 1);
    assert!(s.config().paused.is_empty());
    assert!(s.recovering().is_empty());
    assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_in());
    backend.fail.store(false, Ordering::SeqCst);
    backend.stall.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            s.observe(&c, &storage, &paths, 0)
        )
        .await
        .is_err()
    );
    assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(s.config().paused.is_empty());
    backend.stall.store(false, Ordering::SeqCst);
    s.observe(&c, &storage, &paths, 0).await;
    for _ in 0..6 {
        c.stores.trigger_network_slow_evict(1);
    }
    backend.fail.store(true, Ordering::SeqCst);
    assert_eq!(s.observe(&c, &storage, &paths, 1).await.len(), 1);
    assert!(s.config().evicted.is_empty());
    assert!(!c.stores.get_store(1).unwrap().evicted_as_slow_store());
    backend.fail.store(false, Ordering::SeqCst);
    backend.stall.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            s.observe(&c, &storage, &paths, 2)
        )
        .await
        .is_err()
    );
    assert!(!c.stores.get_store(1).unwrap().evicted_as_slow_store());
    backend.stall.store(false, Ordering::SeqCst);
    s.observe(&c, &storage, &paths, 3).await;
    assert_eq!(s.config().evicted, vec![1]);
    let before = storage
        .load_scheduler_config(&paths, "evict-slow-store-scheduler")
        .await
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(s.update(false, 1, 5, &storage, &paths).await.is_err());
    assert!(!s.config().enabled); // Go retains the enable flag on failed config writes.
    assert!(s.update(true, 1, 5, &storage, &paths).await.is_err());
    assert!(s.config().enabled);
    assert_eq!(s.config().disk.batch(), 3);
    assert_eq!(s.clean(&c, &storage, &paths).await.len(), 1);
    assert_eq!(s.config().paused, vec![1]);
    assert_eq!(s.config().evicted, vec![1]);
    assert!(!c.stores.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(c.stores.get_store(1).unwrap().evicted_as_slow_store());
    score(&c, 1, &[]);
    s.observe(&c, &storage, &paths, 4).await;
    assert_eq!(s.observe(&c, &storage, &paths, 5).await.len(), 1);
    assert_eq!(s.recovering()[&1], Some(4));
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        storage
            .load_scheduler_config(&paths, "evict-slow-store-scheduler")
            .await
            .unwrap(),
        before
    );
    s.observe(&c, &storage, &paths, 6).await;
    assert!(s.config().paused.is_empty());
    assert!(c.stores.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(!c.stores.get_store(1).unwrap().evicted_as_slow_store());
}

use astersql_pd_schedule_core::BasicCluster as _;
const NOW: i128 = 1_000_000_000_000;
#[tokio::test]
async fn task_081_real_operators_and_batches() {
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
        score(&c, 1, &[(2, 95), (3, 10), (4, 10)]);
        for _ in 0..6 {
            c.stores.trigger_network_slow_evict(1);
        }
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
        let mut scheduler =
            EvictNetworkScheduler::new(EvictNetworkConfig::new(true, 0, batch).unwrap());
        let mut ctx = EvictLeaderContext {
            cluster: &c,
            config: &config,
            rules: &mut rules,
            now_unix_nanos: NOW,
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_empty());
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
        score(&c, 1, &[]);
        scheduler.schedule(&mut ctx, &storage, &paths).await;
        let (result, error) = scheduler.schedule(&mut ctx, &storage, &paths).await;
        assert!(error.is_empty());
        assert!(result.operators.is_empty());
    }
    println!("task_081: 4/4 real operator batch cases (1, 3, 5, 100)");
}
