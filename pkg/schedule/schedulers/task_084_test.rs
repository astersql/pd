// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, SlowTrend, StoreInfo, StoreMeta, StoreStats};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::Arc;
const SECOND: i64 = 1_000_000_000;
fn cluster() -> BasicCluster {
    let c = BasicCluster::new();
    for id in 1..=3 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            last_heartbeat: 0,
            ..Default::default()
        })));
        trend(
            &c,
            id,
            if id == 1 { 100. } else { 10. },
            if id == 1 { 1. } else { 0. },
            -1.,
        );
    }
    c
}
fn trend(c: &BasicCluster, id: u64, value: f64, cause: f64, result: f64) {
    let mut s = c.stores.get_store(id).unwrap().as_ref().clone();
    s.set_new_store_stats(StoreStats {
        slow_trend: Some(SlowTrend {
            cause_value: value,
            cause_rate: cause,
            result_rate: result,
            ..Default::default()
        }),
        ..Default::default()
    });
    c.stores.put_store(Arc::new(s));
}
fn heartbeats(c: &BasicCluster, now: i64) {
    for id in 2..=3 {
        let mut s = c.stores.get_store(id).unwrap().as_ref().clone();
        s.set_last_heartbeat_unix_nanos(now);
        c.stores.put_store(Arc::new(s));
    }
}
#[tokio::test]
async fn task_084_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_084.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let c = cluster();
        let paths = ClusterPaths::new(84);
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let mut s = SlowTrendScheduler::new(SlowTrendConfig::default());
        assert!(s.is_schedule_allowed(100, 0));
        assert!(
            !s.observe(&c, &storage, &paths, SECOND, false, 0.0)
                .await
                .unwrap()
        );
        assert_eq!(s.candidates().current.unwrap().store_id, 1);
        assert_eq!(s.config().evicted_store(), 0);
        heartbeats(&c, SECOND);
        assert!(
            s.observe(&c, &storage, &paths, 2 * SECOND, false, 0.0)
                .await
                .unwrap()
        );
        assert_eq!(s.candidates().current, None);
        assert!(c.stores.get_store(1).unwrap().is_evicted_as_slow_trend());
        assert!(!s.is_schedule_allowed(1, 1));
        trend(&c, 1, case["value"].as_f64().unwrap(), 0., 0.);
        let now = case["now"].as_i64().unwrap();
        s.observe(&c, &storage, &paths, now, false, 0.0)
            .await
            .unwrap();
        assert_eq!(
            s.config().evicted_store(),
            case["evicted"].as_u64().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            c.stores.get_store(1).unwrap().is_evicted_as_slow_trend(),
            s.config().evicted_store() != 0
        );
    }
    println!("task_084: 5/5 frozen source-derived recovery cases");
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
async fn task_084_persistence_faults_and_rebuild() {
    let c = cluster();
    let paths = ClusterPaths::new(84);
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let mut s = SlowTrendScheduler::new(SlowTrendConfig::default());
    s.prepare(&c).unwrap();
    s.observe(&c, &storage, &paths, SECOND, false, 0.0)
        .await
        .unwrap();
    heartbeats(&c, SECOND);
    backend.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        s.observe(&c, &storage, &paths, 2 * SECOND, false, 0.0)
            .await,
        Err(SlowTrendError::Scheduler(_))
    ));
    assert_eq!(s.config().evicted_store(), 1); // Go retains mutation on save failure.
    assert_eq!(s.candidates().current, None);
    assert!(!c.stores.get_store(1).unwrap().is_evicted_as_slow_trend());
    assert!(s.update_recovery(0, &storage, &paths).await.is_err());
    assert_eq!(s.config().recovery_sec, 1800);
    backend.fail.store(false, Ordering::SeqCst);
    s.update_recovery(0, &storage, &paths).await.unwrap();
    let saved: SlowTrendConfig = decode_config(
        &storage
            .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved.evicted_store(), 1);
    let c2 = cluster();
    let mut rebuilt = SlowTrendScheduler::new(saved);
    rebuilt.prepare(&c2).unwrap();
    assert!(c2.stores.get_store(1).unwrap().is_evicted_as_slow_trend());
    assert!(rebuilt.prepare(&c2).is_err()); // Duplicate core acquisition is typed.
    assert!(rebuilt.ready_for_recovery(0));
    backend.fail.store(true, Ordering::SeqCst);
    assert!(
        rebuilt
            .clean(&c2, &storage, &paths, 3 * SECOND)
            .await
            .is_err()
    );
    assert_eq!(rebuilt.config().evicted_store(), 0);
    assert!(!c2.stores.get_store(1).unwrap().is_evicted_as_slow_trend());
    backend.fail.store(false, Ordering::SeqCst);
    let saved: SlowTrendConfig = decode_config(
        &storage
            .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved.evicted_store(), 1); // Durable state unchanged by failed clear.
    // Original candidate history is marked even on a failed clear.
    backend.fail.store(true, Ordering::SeqCst);
    assert!(s.clean(&c, &storage, &paths, 4 * SECOND).await.is_err());
    assert_eq!(s.candidates().last.unwrap().recover_ts, 4 * SECOND);
    backend.fail.store(false, Ordering::SeqCst);
    let mut rebuilt = SlowTrendScheduler::new(saved);
    rebuilt.prepare(&c2).unwrap();
    rebuilt
        .clean(&c2, &storage, &paths, 5 * SECOND)
        .await
        .unwrap();
    let saved: SlowTrendConfig = decode_config(
        &storage
            .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved.evicted_store(), 0);
    // JSON and missing store failures are real domain/storage boundary errors.
    for bad in [
        b"{".as_slice(),
        br#"{"recovery-duration":-1}"#,
        br#"{"evict-by-trend-stores":[-1]}"#,
    ] {
        assert!(decode_config::<SlowTrendConfig>(bad).is_err());
    }
    let mut missing = SlowTrendScheduler::new(SlowTrendConfig {
        recovery_sec: 0,
        evicted_stores: vec![99],
    });
    assert!(missing.prepare(&c).is_err());
    assert!(
        !missing
            .observe(&c, &storage, &paths, SECOND, false, 0.0)
            .await
            .unwrap()
    );
    // Restart zero time saturates Go's duration, rather than making u64::MAX ready.
    assert!(
        !SlowTrendScheduler::new(SlowTrendConfig {
            recovery_sec: u64::MAX,
            evicted_stores: vec![1]
        })
        .ready_for_recovery(SECOND)
    );
}
#[tokio::test]
async fn task_084_candidate_lifecycle_reload_and_interval() {
    let c = cluster();
    let paths = ClusterPaths::new(84);
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut s = SlowTrendScheduler::new(SlowTrendConfig::default());
    assert_eq!(s.next_interval(i64::MAX), 13_000_000);
    s.observe(&c, &storage, &paths, SECOND, false, 0.0)
        .await
        .unwrap();
    assert_eq!(
        s.candidates()
            .current
            .unwrap()
            .captured_secs(2 * SECOND - 1),
        0
    );
    trend(&c, 1, 10., 0., 0.);
    assert!(
        !s.observe(&c, &storage, &paths, 2 * SECOND, false, 0.0)
            .await
            .unwrap()
    );
    assert_eq!(s.candidates().current, None);
    assert_eq!(s.candidates().last.unwrap().capture_ts, SECOND);
    trend(&c, 1, 100., 1., -1.);
    s.observe(&c, &storage, &paths, 3 * SECOND, false, 0.0)
        .await
        .unwrap();
    c.stores.delete_store(1);
    assert!(matches!(
        s.observe(&c, &storage, &paths, 4 * SECOND, false, 0.0)
            .await,
        Err(SlowTrendError::MissingCandidate(1))
    ));
    let c = cluster();
    assert!(
        s.reload(
            SlowTrendConfig {
                recovery_sec: 1,
                evicted_stores: vec![2]
            },
            &c
        )
        .is_empty()
    );
    assert!(!c.stores.get_store(2).unwrap().allow_leader_transfer_in());
    assert_eq!(s.next_interval(1), MIN_SCHEDULE_INTERVAL);
    assert!(
        s.reload(
            SlowTrendConfig {
                recovery_sec: 2,
                evicted_stores: vec![3, 99]
            },
            &c
        )
        .len()
            == 1
    );
    assert!(c.stores.get_store(2).unwrap().allow_leader_transfer_in());
    assert_eq!(s.candidates().current.unwrap().store_id, 1); // Reload retains process history.
    assert!(!c.stores.get_store(3).unwrap().is_evicted_as_slow_trend());
    s.prepare(&c).unwrap();
    let mut removed = c.stores.get_store(3).unwrap().as_ref().clone();
    let mut meta = removed.meta().unwrap().clone();
    meta.node_state = astersql_pd_core::NodeState::Removed;
    removed.set_store_meta(&meta);
    c.stores.put_store(Arc::new(removed));
    assert!(
        !s.observe(&c, &storage, &paths, 5 * SECOND, false, 0.0)
            .await
            .unwrap()
    );
    assert_eq!(s.config().evicted_store(), 0);
}

use astersql_pd_schedule_core::BasicCluster as _;
const NOW: i128 = 1_000_000_000_000;
#[tokio::test]
async fn task_084_real_operators() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta, RegionOption};
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    for batch in [3] {
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
        heartbeats(&c, NOW as i64);
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
        let mut scheduler = SlowTrendScheduler::new(SlowTrendConfig {
            recovery_sec: 0,
            ..Default::default()
        });
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
        let (blocked, error) = scheduler
            .schedule(&mut ctx, &storage, &paths, NOW as i64, false, 0.0)
            .await;
        assert!(error.is_none());
        assert!(blocked.operators.is_empty());
        assert_eq!(scheduler.config().evicted_store(), 1);
        for id in [2, 3] {
            c.stores
                .resume_leader_transfer(id, astersql_pd_core_constant::Direction::IN);
        }
        let (result, error) = scheduler
            .schedule(&mut ctx, &storage, &paths, NOW as i64, false, 0.0)
            .await;
        assert!(error.is_none());
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), batch);
        let mut ids = std::collections::HashSet::new();
        for op in result.operators {
            assert!(ids.insert(op.region_id()));
            assert_eq!(op.desc(), "evict-slow-trend-scheduler");
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
        trend(&c, 1, 10., 0., 0.);
        scheduler
            .schedule(&mut ctx, &storage, &paths, NOW as i64, false, 0.0)
            .await;
        let (result, error) = scheduler
            .schedule(&mut ctx, &storage, &paths, NOW as i64, false, 0.0)
            .await;
        assert!(error.is_none());
        assert!(result.operators.is_empty());
    }
    println!("task_084: 1/1 real operator batch case (3)");
}

#[tokio::test]
async fn task_084_cancelled_storage_boundary() {
    let c = cluster();
    let paths = ClusterPaths::new(84);
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let mut s = SlowTrendScheduler::new(SlowTrendConfig::default());
    s.observe(&c, &storage, &paths, SECOND, false, 0.)
        .await
        .unwrap();
    heartbeats(&c, SECOND);
    backend.stall.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            s.observe(&c, &storage, &paths, 2 * SECOND, false, 0.)
        )
        .await
        .is_err()
    );
    assert_eq!(s.config().evicted_store(), 1);
    assert!(!c.stores.get_store(1).unwrap().is_evicted_as_slow_trend());
    backend.stall.store(false, Ordering::SeqCst);
    s.update_recovery(0, &storage, &paths).await.unwrap();
    s.prepare(&c).unwrap();
    backend.stall.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            s.clean(&c, &storage, &paths, 3 * SECOND)
        )
        .await
        .is_err()
    );
    assert_eq!(s.config().evicted_store(), 0);
    assert!(!c.stores.get_store(1).unwrap().is_evicted_as_slow_trend());
    assert_eq!(s.candidates().last.unwrap().recover_ts, 3 * SECOND);
    backend.stall.store(false, Ordering::SeqCst);
    let saved: SlowTrendConfig = decode_config(
        &storage
            .load_scheduler_config(&paths, "evict-slow-trend-scheduler")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(saved.evicted_store(), 1);
}
