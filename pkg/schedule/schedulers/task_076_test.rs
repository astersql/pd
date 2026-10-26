// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster as Cluster, StoreInfo, StoreMeta};
use astersql_pd_core_constant::Direction;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::sync::{Arc, atomic::Ordering};
const NAME: &str = "evict-leader-scheduler";
fn cluster() -> Cluster {
    let c = Cluster::new();
    for id in 0..=3 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    c
}
fn scheduler() -> EvictLeaderScheduler {
    EvictLeaderScheduler::new(EvictLeaderConfig::from_args(&["1", "a", "b"]).unwrap())
}
#[tokio::test]
async fn task_076_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_076.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let c = cluster();
        let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let paths = ClusterPaths::new(76);
        let mut s = scheduler();
        s.prepare(&c).unwrap();
        let before = s.encode_evict_config().unwrap();
        storage
            .save_scheduler_config(&paths, NAME, &before)
            .await
            .unwrap();
        let result = s
            .update_evict_config(&case["request"], &c, &storage, &paths)
            .await;
        assert_eq!(
            result.is_ok(),
            case["ok"].as_bool().unwrap(),
            "{}: {result:?}",
            case["name"]
        );
        assert_eq!(s.config().batch(), case["batch"].as_u64().unwrap() as usize);
        let expected: Vec<_> = case["stores"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(
            s.config().stores().keys().copied().collect::<Vec<_>>(),
            expected,
            "{}",
            case["name"]
        );
        for id in 0..=3 {
            assert_eq!(
                !c.get_store(id).unwrap().allow_leader_transfer_in(),
                expected.contains(&id)
            );
        }
        if result.is_err() {
            assert_eq!(s.encode_evict_config().unwrap(), before);
        }
        if case["name"] == "explicit" {
            assert_eq!(s.config().stores()[&1][0].start_key, b"c");
            assert_eq!(s.config().stores()[&2][0].end_key, b"d");
        } else {
            assert_eq!(s.config().stores()[&1][0].start_key, b"a");
        }
        assert_eq!(
            storage.load_scheduler_config(&paths, NAME).await.unwrap(),
            s.encode_evict_config().unwrap()
        );
        let snapshot = s.config().clone();
        s.reload_evict_config(&c, &storage, &paths).await.unwrap();
        assert_eq!(s.config().stores(), snapshot.stores());
        s.clean(&c);
        for id in expected {
            assert!(c.get_store(id).unwrap().allow_leader_transfer_in());
        }
        println!("task_076 {} passed", case["name"]);
    }
    println!(
        "task_076: {}/{} source-derived update cases passed",
        cases.len(),
        cases.len()
    );
}
#[tokio::test]
async fn task_076_transactions_and_faults() {
    let c = cluster();
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(76);
    let mut s = scheduler();
    s.prepare(&c).unwrap();
    c.pause_leader_transfer(2, Direction::IN).unwrap();
    c.pause_leader_transfer(1, Direction::OUT).unwrap();
    let before = s.encode_evict_config().unwrap();
    storage
        .save_scheduler_config(&paths, NAME, &before)
        .await
        .unwrap();
    backend.stall.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            s.update_evict_config(
                &serde_json::json!({"store_ids":[1,2],"batch":5}),
                &c,
                &storage,
                &paths
            )
        )
        .await
        .is_err()
    );
    backend.stall.store(false, Ordering::SeqCst);
    assert_eq!(s.encode_evict_config().unwrap(), before);
    c.resume_leader_transfer(2, Direction::IN);
    assert!(c.get_store(2).unwrap().allow_leader_transfer_in());
    c.pause_leader_transfer(2, Direction::IN).unwrap();
    assert_eq!(
        storage.load_scheduler_config(&paths, NAME).await.unwrap(),
        before
    );
    for fault in [false, true] {
        backend.fail.store(fault, Ordering::SeqCst);
        let input = if fault {
            serde_json::json!({"store_ids":[1,2],"ranges":["c","d"],"batch":5})
        } else {
            serde_json::json!({"store_ids":[1,2,999],"ranges":["c","d"],"batch":5})
        };
        let result = s.update_evict_config(&input, &c, &storage, &paths).await;
        if fault {
            assert!(matches!(
                result,
                Err(EvictConfigError::Scheduler(SchedulerError::Storage(_)))
            ));
        } else {
            assert!(matches!(
                result,
                Err(EvictConfigError::Pause(
                    astersql_pd_core::StoreControlError::StoreNotFound(999)
                ))
            ));
        }
        assert_eq!(s.encode_evict_config().unwrap(), before);
        assert!(!c.get_store(2).unwrap().allow_leader_transfer_in());
        c.resume_leader_transfer(2, Direction::IN);
        assert!(c.get_store(2).unwrap().allow_leader_transfer_in());
        c.pause_leader_transfer(2, Direction::IN).unwrap();
        assert!(!c.get_store(1).unwrap().allow_leader_transfer_out());
    }
    assert!(s.reload_evict_config(&c, &storage, &paths).await.is_err());
    assert_eq!(s.encode_evict_config().unwrap(), before);
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        storage.load_scheduler_config(&paths, NAME).await.unwrap(),
        before
    );
    s.update_evict_config(
        &serde_json::json!({"store_ids":[1,2],"batch":5}),
        &c,
        &storage,
        &paths,
    )
    .await
    .unwrap();
    let saved = s.encode_evict_config().unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(
        s.delete_evict_store(1, &c, &storage, &paths, |_| panic!("not last"))
            .await
            .is_err()
    );
    assert_eq!(s.encode_evict_config().unwrap(), saved);
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_in());
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        s.delete_evict_store(1, &c, &storage, &paths, |_| panic!("not last"))
            .await
            .unwrap(),
        None
    );
    assert!(c.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(matches!(
        s.delete_evict_store(99, &c, &storage, &paths, |_| panic!())
            .await,
        Err(EvictConfigError::MissingStore(99))
    ));
    assert!(matches!(
        s.delete_evict_store(2, &c, &storage, &paths, |name| {
            assert_eq!(name, NAME);
            Err(EvictRemoveError::Failed("callback".into()))
        })
        .await,
        Err(EvictConfigError::Remove(EvictRemoveError::Failed(_)))
    ));
    assert!(s.config().stores().contains_key(&2));
    // Go restores the in-memory config but does not rewrite the saved empty map.
    let persisted: EvictLeaderConfig =
        decode_config(&storage.load_scheduler_config(&paths, NAME).await.unwrap()).unwrap();
    assert!(persisted.stores().is_empty());
    assert!(matches!(
        s.delete_evict_store(2, &c, &storage, &paths, |_| Err(EvictRemoveError::NotFound))
            .await,
        Err(EvictConfigError::Remove(EvictRemoveError::NotFound))
    ));
    assert!(s.config().stores().is_empty());
    c.resume_leader_transfer(2, Direction::IN);
    assert!(c.get_store(2).unwrap().allow_leader_transfer_in());
    s.update_evict_config(&serde_json::json!({"store_id":3}), &c, &storage, &paths)
        .await
        .unwrap();
    assert_eq!(
        s.delete_evict_store(3, &c, &storage, &paths, |_| Ok(()))
            .await
            .unwrap(),
        Some("The last store has been deleted")
    );
    println!(
        "pause/save rollback, delete save/callback rollback, not-found and successful removal passed"
    );
}
#[tokio::test]
async fn task_076_args_reload_and_boundaries() {
    for args in [
        vec![],
        vec!["x"],
        vec!["1,x"],
        vec!["1,"],
        vec!["-1"],
        vec!["+1"],
        vec!["18446744073709551616"],
        vec!["1,2", "a", "b"],
        vec!["1", "%zz", "b"],
    ] {
        assert!(EvictLeaderConfig::from_args(&args).is_err(), "{args:?}");
    }
    for (args, ids) in [
        (vec!["1"], vec![1]),
        (vec!["1,2,2,3"], vec![1, 2, 3]),
        (vec!["18446744073709551615"], vec![u64::MAX]),
    ] {
        assert_eq!(
            EvictLeaderConfig::from_args(&args)
                .unwrap()
                .stores()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            ids
        );
    }
    for f in [f64::NAN, f64::INFINITY, -1., 1.5, 1e30] {
        assert!(evict_store_id_from_float(f).is_err());
    }
    assert_eq!(evict_store_id_from_float(0.).unwrap(), 0);
    // Go permits the rounded upper bound; its float->uint conversion is unspecified.
    // Rust rejects the non-representable 2^64 value, rather than saturating it.
    assert!(evict_store_id_from_float(18446744073709551616.).is_err());
    let c = cluster();
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(76);
    let mut s = scheduler();
    s.prepare(&c).unwrap();
    let before = s.encode_evict_config().unwrap();
    for bytes in [
        b"{".as_slice(),
        br#"{"store-id-ranges":{},"batch":101}"#,
        br#"{"store-id-ranges":{},"batch":1.5}"#,
    ] {
        storage
            .save_scheduler_config(&paths, NAME, bytes)
            .await
            .unwrap();
        assert!(s.reload_evict_config(&c, &storage, &paths).await.is_err());
        assert_eq!(s.encode_evict_config().unwrap(), before);
        assert!(!c.get_store(1).unwrap().allow_leader_transfer_in());
    }
    storage
        .save_scheduler_config(
            &paths,
            NAME,
            br#"{"store-id-ranges":{"2":[{"start-key":"","end-key":""}]},"batch":0}"#,
        )
        .await
        .unwrap();
    assert!(
        s.reload_evict_config(&c, &storage, &paths)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(s.config().batch(), 3);
    assert_eq!(
        s.config().stores().keys().copied().collect::<Vec<_>>(),
        vec![2]
    );
    assert!(c.get_store(1).unwrap().allow_leader_transfer_in());
    assert!(!c.get_store(2).unwrap().allow_leader_transfer_in());
    // Retained IDs must not gain a second pause during reload.
    s.reload_evict_config(&c, &storage, &paths).await.unwrap();
    s.clean(&c);
    assert!(c.get_store(2).unwrap().allow_leader_transfer_in());
    storage
        .save_scheduler_config(&paths, NAME, br#"{"store-id-ranges":{"999":[]}}"#)
        .await
        .unwrap();
    assert_eq!(
        s.reload_evict_config(&c, &storage, &paths).await.unwrap(),
        vec![astersql_pd_core::StoreControlError::StoreNotFound(999)]
    );
    assert_eq!(s.config().batch(), 3);
    assert!(s.config().stores().contains_key(&999));
    let before_missing = s.encode_evict_config().unwrap();
    assert!(
        s.reload_evict_config(&c, &storage, &ClusterPaths::new(760))
            .await
            .is_err()
    );
    assert_eq!(s.encode_evict_config().unwrap(), before_missing);
    println!(
        "creation args, float bounds, malformed reloads, default batch and pause reconciliation passed"
    );
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
    Commit, Compare, Lease, MemoryStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read,
    Snapshot, StoreError,
};
