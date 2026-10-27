// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{StoreInfo, StoreMeta};
use std::sync::Arc;
fn store(id: u64, count: i64) -> Arc<StoreInfo> {
    let mut s = StoreInfo::new(StoreMeta {
        id,
        ..Default::default()
    });
    s.set_leader_count(count);
    s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
    Arc::new(s)
}
#[tokio::test]
async fn task_071_behavior_cases() {
    use astersql_pd_core::{BasicCluster as CoreCluster, KeyRange, Peer, RegionInfo, RegionMeta};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::AffinityFilter;
    use astersql_pd_schedule_operator::{OpInfluence, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_statistics::StatisticsSnapshot;
    use astersql_pd_storage_domain::MemoryStore;
    use astersql_pd_storage_domain::StorageEndpoint;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_071.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let counts = case["counts"]
            .as_array()
            .unwrap_or(fixture["defaults"]["counts"].as_array().unwrap());
        let sizes = case["sizes"]
            .as_array()
            .unwrap_or(fixture["defaults"]["sizes"].as_array().unwrap());
        let mut core = CoreCluster::new();
        for id in 1..=3 {
            let mut st = (*store(id, counts[id as usize - 1].as_i64().unwrap())).clone();
            st.set_leader_size(sizes[id as usize - 1].as_i64().unwrap());
            core.stores.put_store(Arc::new(st));
        }
        for id in 1..=8 {
            let peers: Vec<_> = (1..=3)
                .map(|store_id| Peer {
                    id: id * 10 + store_id,
                    store_id,
                    ..Default::default()
                })
                .collect();
            let mut region = RegionInfo::new(
                RegionMeta {
                    id,
                    key_range: KeyRange {
                        start_key: vec![b'a' + id as u8 - 1],
                        end_key: vec![b'a' + id as u8],
                    },
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[0].clone()),
                vec![],
                vec![],
            );
            region.stats_mut().approximate_size = 10;
            let region = Arc::new(region);
            let (o, v, b) = core.regions.set_region(region.clone());
            core.regions.update_sub_tree(region, o.as_deref(), &v, b);
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.tolerant_size_ratio = 1.0;
        config.schedule.leader_schedule_policy = "count".into();
        config.replication.enable_placement_rules = false;
        let storage = Arc::new(MemoryStore::default());
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(storage.clone())),
            ClusterPaths::new(71),
        );
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
            StorageEndpoint::new(storage),
            ClusterPaths::new(71),
            Default::default(),
        )
        .await
        .unwrap();
        let key_ranges = astersql_pd_schedule_keyrange::Manager::new();
        let influence = OpInfluence::new();
        let before = influence.snapshot();
        let statistics = StatisticsSnapshot::default();
        let mut next = 0usize;
        let mut index = |n: usize| {
            let result = (next / 20) % n;
            next += 1;
            result
        };
        let mut permutations = 0usize;
        let mut shuffle = |order: &mut [usize]| {
            let n = order.len();
            order.rotate_left(permutations % n);
            permutations += 1;
        };

        let endpoint = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
        let paths = ClusterPaths::new(71);
        let mut scheduler = BalanceLeaderScheduler::default();
        let ranges = case["ranges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| KeyRange {
                start_key: r[0].as_str().unwrap().as_bytes().to_vec(),
                end_key: r[1].as_str().unwrap().as_bytes().to_vec(),
            })
            .collect();
        let candidate =
            BalanceLeaderConfig::new(ranges, case["batch"].as_u64().unwrap() as usize).unwrap();
        let original = scheduler.encode_leader_config().unwrap();
        let outcome = scheduler
            .update_leader_config(candidate.clone(), &endpoint, &paths)
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            LeaderConfigUpdate::Updated {
                persistence_error: None
            }
        ));
        assert_ne!(scheduler.encode_leader_config().unwrap(), original);
        let encoded = scheduler.encode_leader_config().unwrap();
        let actual: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(actual["batch"], case["batch"]);
        for (actual, expected) in actual["ranges"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["ranges"].as_array().unwrap())
        {
            let hex = |s: &str| {
                s.as_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            };
            assert_eq!(actual["start-key"], hex(expected[0].as_str().unwrap()));
            assert_eq!(actual["end-key"], hex(expected[1].as_str().unwrap()));
        }
        assert!(matches!(
            scheduler
                .update_leader_config(candidate, &endpoint, &paths)
                .await
                .unwrap(),
            LeaderConfigUpdate::Unchanged
        ));
        let mut restored = BalanceLeaderScheduler::default();
        restored
            .reload_leader_config(&endpoint, &paths)
            .await
            .unwrap();
        assert_eq!(restored.encode_leader_config().unwrap(), encoded);
        let mut ctx = LeaderBalanceContext {
            cluster: &core,
            config: &config,
            rules: &mut rules,
            affinity: AffinityFilter {
                manager: None,
                labels: &labels,
                now: 1_000_000_000_000,
            },
            statistics: &statistics,
            hot_degree: 3,
            key_ranges: &key_ranges,
            influence: &influence,
            now_unix_nanos: 1_000_000_000_000,
            random_index: &mut index,
            shuffle: &mut shuffle,
        };
        let mut original_scheduler = BalanceLeaderScheduler::default();
        assert_eq!(
            original_scheduler
                .schedule(&mut ctx, false)
                .await
                .operators
                .len(),
            4
        );
        for active in [&mut scheduler, &mut restored] {
            let result = active.schedule(&mut ctx, false).await;
            assert_eq!(
                result.operators.len(),
                case["expected"].as_u64().unwrap() as usize,
                "{name}"
            );
            assert!(result.errors.is_empty());
            let mut ids = std::collections::HashSet::new();
            for op in result.operators {
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!("wrong command")
                };
                assert_eq!(step.from_store, 1);
                assert!([2, 3].contains(&step.to_store));
                assert!(ids.insert(op.region_id()));
                let start = vec![b'a' + op.region_id() as u8 - 1];
                let end = vec![b'a' + op.region_id() as u8];
                assert!(active.config.ranges().iter().any(|r|
                    r.start_key <= start && (r.end_key.is_empty() || end <= r.end_key)));
            }
        }
        assert_eq!(influence.snapshot(), before);
    }
    println!("6/6 frozen config-to-decision and restored-decision cases passed");
}

use astersql_pd_storage_domain::{
    Commit, Compare, Lease, MemoryStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read,
    Snapshot, StoreError,
};
#[tokio::test]
async fn task_071_faults_and_validation() {
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_storage_domain::StorageEndpoint;
    use std::sync::atomic::Ordering;
    let backend = Arc::new(FaultStore::default());
    let endpoint = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(71);
    let mut scheduler = BalanceLeaderScheduler::default();
    let baseline = scheduler.encode_leader_config().unwrap();
    endpoint
        .save_scheduler_config(&paths, &scheduler.name, &baseline)
        .await
        .unwrap();
    for batch in [0, 101, usize::MAX] {
        assert!(scheduler.config.set_batch(batch).is_err());
        assert_eq!(scheduler.encode_leader_config().unwrap(), baseline);
    }
    let mut clone = scheduler.config.clone();
    clone.set_batch(100).unwrap();
    assert_eq!(scheduler.config.batch(), 4);
    backend.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        scheduler
            .update_leader_config(clone, &endpoint, &paths)
            .await
            .unwrap(),
        LeaderConfigUpdate::Updated {
            persistence_error: Some(_)
        }
    ));
    assert_eq!(
        scheduler.config.batch(),
        100,
        "Go keeps valid update active when persistence fails"
    );
    assert!(
        scheduler
            .reload_leader_config(&endpoint, &paths)
            .await
            .is_err()
    );
    assert_eq!(scheduler.config.batch(), 100);
    // Same-value updates don't touch storage, even while the backend is unavailable.
    assert!(matches!(
        scheduler
            .update_leader_config(scheduler.config.clone(), &endpoint, &paths)
            .await
            .unwrap(),
        LeaderConfigUpdate::Unchanged
    ));
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        endpoint
            .load_scheduler_config(&paths, &scheduler.name)
            .await
            .unwrap(),
        baseline
    );
    scheduler
        .reload_leader_config(&endpoint, &paths)
        .await
        .unwrap();
    assert_eq!(scheduler.config.batch(), 4);
    for data in [
        b"".as_slice(),
        b"{",
        b"null",
        br#"{"ranges":[],"batch":0}"#,
        br#"{"ranges":[],"batch":101}"#,
        br#"{"ranges":[{"start-key":"bad"}],"batch":4}"#,
    ] {
        endpoint
            .save_scheduler_config(&paths, &scheduler.name, data)
            .await
            .unwrap();
        assert!(
            scheduler
                .reload_leader_config(&endpoint, &paths)
                .await
                .is_err()
        );
        assert_eq!(scheduler.encode_leader_config().unwrap(), baseline);
    }
    scheduler.name = "custom-leader".into();
    assert!(
        scheduler
            .reload_leader_config(&endpoint, &paths)
            .await
            .is_err()
    );
    let new = BalanceLeaderConfig::new(vec![], 1).unwrap();
    scheduler
        .update_leader_config(new, &endpoint, &paths)
        .await
        .unwrap();
    assert_eq!(scheduler.config.batch(), 1);
    assert_eq!(
        endpoint
            .load_scheduler_config(&paths, "custom-leader")
            .await
            .unwrap(),
        scheduler.encode_leader_config().unwrap()
    );
}
/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
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
