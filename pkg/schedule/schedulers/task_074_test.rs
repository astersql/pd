// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster as CoreCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::sync::Arc;

#[tokio::test]
async fn task_074_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_074.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut cluster = CoreCluster::new();
        for (id, count) in [(1, 7), (2, 8), (3, 9), (4, 14)] {
            let mut s = StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            });
            s.set_witness_count(count);
            s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
            cluster.stores.put_store(Arc::new(s));
        }
        let peers: Vec<_> = (1..=4)
            .map(|id| Peer {
                id: id + 10,
                store_id: id,
                is_witness: id == 4,
                ..Default::default()
            })
            .collect();
        let region = Arc::new(RegionInfo::new(
            RegionMeta {
                id: 1,
                key_range: KeyRange {
                    start_key: b"a".to_vec(),
                    end_key: b"b".to_vec(),
                },
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[2].clone()),
            vec![],
            vec![],
        ));
        let (o, v, b) = cluster.regions.set_region(region.clone());
        cluster.regions.update_sub_tree(region, o.as_deref(), &v, b);
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.tolerant_size_ratio = 2.5;
        config.replication.max_replicas = 4;
        config.replication.enable_placement_rules = false;
        let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(74);
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            paths.clone(),
        );
        rules
            .initialize(4, &[], "", false, &config, &[])
            .await
            .unwrap();
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
            BalanceWitnessConfig::new(ranges, case["batch"].as_u64().unwrap() as usize).unwrap();
        let mut active = BalanceWitnessScheduler::default();
        assert!(
            active
                .update_witness_config(candidate.clone(), &endpoint, &paths)
                .await
                .unwrap()
        );
        let encoded = active.encode_witness_config().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(json["batch"], case["batch"]);
        assert_eq!(
            json["ranges"].as_array().unwrap().len(),
            case["ranges"].as_array().unwrap().len().max(1)
        );
        let decoded: BalanceWitnessConfig = decode_config(&encoded).unwrap();
        assert_eq!(decoded.ranges(), candidate.ranges());
        assert!(
            !active
                .update_witness_config(candidate, &endpoint, &paths)
                .await
                .unwrap()
        );
        assert_eq!(
            endpoint
                .load_scheduler_config(&paths, "balance-witness-scheduler")
                .await
                .unwrap(),
            encoded
        );
        let mut restored = BalanceWitnessScheduler::default();
        restored
            .reload_witness_config(&endpoint, &paths)
            .await
            .unwrap();
        assert_eq!(restored.encode_witness_config().unwrap(), encoded);
        let influence = OpInfluence::new();
        let mut random = |_: usize| 0;
        let mut shuffle = |_: &mut [usize]| {};
        let mut ctx = WitnessBalanceContext {
            cluster: &cluster,
            config: &config,
            rules: &mut rules,
            influence: &influence,
            now_unix_nanos: 1_000_000_000_000,
            random_index: &mut random,
            shuffle: &mut shuffle,
            supports_conf_change_v2: true,
        };
        for scheduler in [&mut active, &mut restored] {
            let result = scheduler.schedule(&mut ctx, true).await;
            assert_eq!(
                result.operators.len(),
                case["expected"].as_u64().unwrap() as usize,
                "{}",
                case["name"]
            );
            assert!(result.errors.is_empty());
            for op in result.operators {
                assert_eq!(op.region_id(), 1);
                assert_eq!(op.kind(), OpKind::WITNESS | OpKind::REGION);
                let mut to_witness = Vec::new();
                let mut to_non_witness = Vec::new();
                for i in 0..op.len() {
                    match op.step(i).unwrap() {
                        OpStep::BatchSwitchWitness(s) => {
                            to_witness.extend(s.to_witnesses.iter().map(|p| p.store_id));
                            to_non_witness.extend(s.to_non_witnesses.iter().map(|p| p.store_id));
                        }
                        OpStep::BecomeWitness(s) => to_witness.push(s.store_id),
                        OpStep::BecomeNonWitness(s) => to_non_witness.push(s.store_id),
                        _ => {}
                    }
                }
                assert_eq!(to_witness, vec![1]);
                assert_eq!(to_non_witness, vec![4]);
            }
        }
    }
    println!("6/6 configuration-to-decision and restored-decision cases passed");
}

#[tokio::test]
async fn task_074_faults_and_validation() {
    use std::sync::atomic::Ordering;
    let backend = Arc::new(FaultStore::default());
    let endpoint = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(74);
    let mut scheduler = BalanceWitnessScheduler::default();
    let baseline = scheduler.encode_witness_config().unwrap();
    endpoint
        .save_scheduler_config(&paths, "balance-witness-scheduler", &baseline)
        .await
        .unwrap();
    for batch in [0, 11, usize::MAX] {
        assert!(BalanceWitnessConfig::new(vec![], batch).is_err());
        assert_eq!(scheduler.encode_witness_config().unwrap(), baseline);
    }
    backend.fail.store(true, Ordering::SeqCst);
    let candidate = BalanceWitnessConfig::new(
        vec![KeyRange {
            start_key: b"c".to_vec(),
            end_key: b"d".to_vec(),
        }],
        10,
    )
    .unwrap();
    assert!(matches!(
        scheduler
            .update_witness_config(candidate.clone(), &endpoint, &paths)
            .await,
        Err(SchedulerError::Storage(_))
    ));
    assert_eq!(
        scheduler.encode_witness_config().unwrap(),
        baseline,
        "task contract: failed save must retain batch AND ranges"
    );
    assert!(matches!(
        scheduler.reload_witness_config(&endpoint, &paths).await,
        Err(SchedulerError::Storage(_))
    ));
    assert_eq!(scheduler.encode_witness_config().unwrap(), baseline);
    // An identical value does not touch unavailable storage.
    assert!(
        !scheduler
            .update_witness_config(scheduler.config.clone(), &endpoint, &paths)
            .await
            .unwrap()
    );
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        endpoint
            .load_scheduler_config(&paths, "balance-witness-scheduler")
            .await
            .unwrap(),
        baseline
    );
    assert!(
        scheduler
            .update_witness_config(candidate, &endpoint, &paths)
            .await
            .unwrap()
    );
    let saved = scheduler.encode_witness_config().unwrap();
    scheduler.config = BalanceWitnessConfig::default();
    scheduler
        .reload_witness_config(&endpoint, &paths)
        .await
        .unwrap();
    assert_eq!(scheduler.encode_witness_config().unwrap(), saved);
    for bytes in [
        b"".as_slice(),
        b"{",
        b"null",
        br#"{"ranges":[],"batch":0}"#,
        br#"{"ranges":[],"batch":11}"#,
        br#"{"ranges":[],"batch":-1}"#,
        br#"{"ranges":[],"batch":1.5}"#,
        br#"{"ranges":[],"batch":"4"}"#,
        br#"{"ranges":[{"start-key":"bad"}],"batch":4}"#,
    ] {
        endpoint
            .save_scheduler_config(&paths, "balance-witness-scheduler", bytes)
            .await
            .unwrap();
        assert!(matches!(
            scheduler.reload_witness_config(&endpoint, &paths).await,
            Err(SchedulerError::Decode(_))
        ));
        assert_eq!(scheduler.encode_witness_config().unwrap(), saved);
    }
    assert!(
        scheduler
            .reload_witness_config(&endpoint, &ClusterPaths::new(740))
            .await
            .is_err()
    );
    assert_eq!(scheduler.encode_witness_config().unwrap(), saved);
    let binary = BalanceWitnessConfig::new(
        vec![KeyRange {
            start_key: vec![0, 255],
            end_key: vec![255, 0],
        }],
        1,
    )
    .unwrap();
    scheduler
        .update_witness_config(binary.clone(), &endpoint, &paths)
        .await
        .unwrap();
    scheduler.config = BalanceWitnessConfig::default();
    scheduler
        .reload_witness_config(&endpoint, &paths)
        .await
        .unwrap();
    assert_eq!(scheduler.config.ranges(), binary.ranges());
    assert_eq!(scheduler.config.batch(), 1);
    println!(
        "3 invalid constructors, save/read faults, unchanged no-I/O, retry, 9 invalid reloads, missing key, binary keys passed"
    );
}

/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
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
    Commit, Compare, Lease, MemoryStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read,
    Snapshot, StoreError,
};
