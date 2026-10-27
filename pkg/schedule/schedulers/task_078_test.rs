// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster as Cluster, StoreInfo, StoreMeta};
use astersql_pd_core_constant::Direction;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::{Arc, atomic::Ordering};
const NAME: &str = "grant-leader-scheduler";
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
fn scheduler() -> GrantLeaderScheduler {
    GrantLeaderScheduler::new(GrantLeaderConfig::from_args(&["1", "a", "b"]).unwrap())
}
#[test]
fn task_078_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_078.json"
    ))
    .unwrap();
    let cases = f["cases"].as_array().unwrap();
    for case in cases {
        let args: Vec<_> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let result = GrantLeaderConfig::from_args(&args);
        assert_eq!(result.is_ok(), case["ok"].as_bool().unwrap(), "{case}");
        if let Ok(conf) = result {
            let ranges = &conf.stores()[&case["id"].as_u64().unwrap()];
            let expected = case["ranges"].as_array().unwrap();
            assert_eq!(ranges.len(), expected.len());
            for (r, e) in ranges.iter().zip(expected) {
                assert_eq!(r.start_key, e[0].as_str().unwrap().as_bytes());
                let end = if e[1] == "ff:ff" {
                    vec![255]
                } else {
                    e[1].as_str().unwrap().as_bytes().to_vec()
                };
                assert_eq!(r.end_key, end);
            }
            let encoded = encode_config(&conf).unwrap();
            let decoded: GrantLeaderConfig = decode_config(&encoded).unwrap();
            assert_eq!(decoded.stores(), conf.stores());
        }
    }
    println!(
        "task_078: {}/{} frozen source-derived args cases passed",
        cases.len(),
        cases.len()
    );
}
#[tokio::test]
async fn task_078_updates_and_faults() {
    let c = cluster();
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(78);
    let mut s = scheduler();
    s.prepare(&c).unwrap();
    c.pause_leader_transfer(1, Direction::IN).unwrap();
    s.update_grant_config(1, None, &c, &storage, &paths)
        .await
        .unwrap();
    assert_eq!(s.config().stores()[&1][0].start_key, b"a");
    s.update_grant_config(2, Some(&["c", "d"]), &c, &storage, &paths)
        .await
        .unwrap();
    assert_eq!(s.config().stores()[&2][0].end_key, b"d");
    s.clean(&c);
    assert!(c.get_store(1).unwrap().allow_leader_transfer_out());
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_in());
    s.prepare(&c).unwrap();
    let before = s.encode_grant_config().unwrap();
    assert!(matches!(
        s.update_grant_config(99, None, &c, &storage, &paths).await,
        Err(GrantConfigError::Pause(_))
    ));
    assert_eq!(s.encode_grant_config().unwrap(), before);
    assert!(
        s.update_grant_config(3, Some(&["%zz", "b"]), &c, &storage, &paths)
            .await
            .is_err()
    );
    assert!(c.get_store(3).unwrap().allow_leader_transfer_out());
    assert_eq!(s.encode_grant_config().unwrap(), before);
    backend.fail.store(true, Ordering::SeqCst);
    // Both new and existing stores are removed on failed persistence, as Go does.
    for id in [3, 1] {
        assert!(matches!(
            s.update_grant_config(id, Some(&["e", "f"]), &c, &storage, &paths)
                .await,
            Err(GrantConfigError::Scheduler(SchedulerError::Storage(_)))
        ));
        assert!(!s.config().stores().contains_key(&id));
        assert!(c.get_store(id).unwrap().allow_leader_transfer_out());
    }
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_in());
    let retained = s.encode_grant_config().unwrap();
    assert!(s.reload_grant_config(&c, &storage, &paths).await.is_err());
    assert_eq!(s.encode_grant_config().unwrap(), retained);
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        storage.load_scheduler_config(&paths, NAME).await.unwrap(),
        before
    );
    s.reload_grant_config(&c, &storage, &paths).await.unwrap();
    assert_eq!(s.encode_grant_config().unwrap(), before);
    // Cancellation before storage completion retains the published snapshot and releases new pauses.
    backend.stall.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            s.update_grant_config(3, None, &c, &storage, &paths)
        )
        .await
        .is_err()
    );
    backend.stall.store(false, Ordering::SeqCst);
    assert_eq!(s.encode_grant_config().unwrap(), before);
    assert!(c.get_store(3).unwrap().allow_leader_transfer_out());
    assert_eq!(
        storage.load_scheduler_config(&paths, NAME).await.unwrap(),
        before
    );
    s.clean(&c);
}
#[tokio::test]
async fn task_078_delete_and_reload() {
    let c = cluster();
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(78);
    let mut s = scheduler();
    s.prepare(&c).unwrap();
    let before = s.encode_grant_config().unwrap();
    storage
        .save_scheduler_config(&paths, NAME, &before)
        .await
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert!(
        s.delete_grant_store(1, &c, &storage, &paths, |_| panic!())
            .await
            .is_err()
    );
    assert_eq!(s.encode_grant_config().unwrap(), before);
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_out());
    backend.fail.store(false, Ordering::SeqCst);
    assert!(matches!(
        s.delete_grant_store(99, &c, &storage, &paths, |_| panic!())
            .await,
        Err(GrantConfigError::MissingStore(99))
    ));
    assert!(matches!(
        s.delete_grant_store(1, &c, &storage, &paths, |n| {
            assert_eq!(n, NAME);
            Err(GrantRemoveError::Failed("callback".into()))
        })
        .await,
        Err(GrantConfigError::Remove(GrantRemoveError::Failed(_)))
    ));
    assert_eq!(s.encode_grant_config().unwrap(), before);
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_out());
    let saved: GrantLeaderConfig =
        decode_config(&storage.load_scheduler_config(&paths, NAME).await.unwrap()).unwrap();
    assert!(saved.stores().is_empty());
    assert!(matches!(
        s.delete_grant_store(1, &c, &storage, &paths, |_| Err(GrantRemoveError::NotFound))
            .await,
        Err(GrantConfigError::Remove(GrantRemoveError::NotFound))
    ));
    assert!(s.config().stores().is_empty());
    assert!(c.get_store(1).unwrap().allow_leader_transfer_out());
    s.update_grant_config(1, None, &c, &storage, &paths)
        .await
        .unwrap();
    s.update_grant_config(2, None, &c, &storage, &paths)
        .await
        .unwrap();
    assert_eq!(
        s.delete_grant_store(1, &c, &storage, &paths, |_| panic!())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        s.delete_grant_store(2, &c, &storage, &paths, |_| Ok(()))
            .await
            .unwrap(),
        Some("The last store has been deleted")
    );
    s.update_grant_config(1, None, &c, &storage, &paths)
        .await
        .unwrap();
    let before = s.encode_grant_config().unwrap();
    for bytes in [
        b"{".as_slice(),
        br#"{"store-id-ranges":{"x":[]}}"#,
        br#"{"store-id-ranges":{"2":[{"start-key":1}]}}"#,
    ] {
        storage
            .save_scheduler_config(&paths, NAME, bytes)
            .await
            .unwrap();
        assert!(s.reload_grant_config(&c, &storage, &paths).await.is_err());
        assert_eq!(s.encode_grant_config().unwrap(), before);
        assert!(!c.get_store(1).unwrap().allow_leader_transfer_out());
    }
    storage
        .save_scheduler_config(
            &paths,
            NAME,
            br#"{"store-id-ranges":{"2":[{"start-key":"61","end-key":"62"}],"99":[]}}"#,
        )
        .await
        .unwrap();
    assert_eq!(
        s.reload_grant_config(&c, &storage, &paths).await.unwrap(),
        vec![astersql_pd_core::StoreControlError::StoreNotFound(99)]
    );
    assert!(c.get_store(1).unwrap().allow_leader_transfer_out());
    assert!(!c.get_store(2).unwrap().allow_leader_transfer_out());
    s.reload_grant_config(&c, &storage, &paths).await.unwrap();
    s.clean(&c);
    assert!(c.get_store(2).unwrap().allow_leader_transfer_out());
    let before = s.encode_grant_config().unwrap();
    assert!(
        s.reload_grant_config(&c, &storage, &ClusterPaths::new(780))
            .await
            .is_err()
    );
    assert_eq!(s.encode_grant_config().unwrap(), before);
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

#[test]
fn task_078_reset_remove_and_schema() {
    let c = cluster();
    let mut conf = GrantLeaderConfig::default();
    assert_eq!(conf.remove_store(1, &c), (false, false));
    conf.reset_store(1, vec![astersql_pd_core::KeyRange::default()], &c)
        .unwrap();
    conf.reset_store(2, vec![], &c).unwrap();
    assert!(!c.get_store(1).unwrap().allow_leader_transfer_out());
    assert_eq!(conf.remove_store(1, &c), (true, false));
    assert!(c.get_store(1).unwrap().allow_leader_transfer_out());
    assert_eq!(conf.remove_store(2, &c), (true, true));
    assert!(matches!(
        conf.reset_store(99, vec![], &c),
        Err(astersql_pd_core::StoreControlError::StoreNotFound(99))
    ));
    assert!(conf.stores().contains_key(&99));
    assert_eq!(conf.remove_store(99, &c), (true, true));
    for bytes in [br#"{}"#.as_slice(), br#"{"store-id-ranges":null}"#] {
        let conf: GrantLeaderConfig = decode_config(bytes).unwrap();
        assert!(conf.stores().is_empty());
    }
    let conf: GrantLeaderConfig =
        decode_config(br#"{"store-id-ranges":{"1":null,"2":[]},"unknown":true}"#).unwrap();
    assert!(conf.stores()[&1].is_empty());
    assert!(conf.stores()[&2].is_empty());
    let conf = GrantLeaderConfig::from_args(&["1", "%00%ff", "a+b"]).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&encode_config(&conf).unwrap()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"store-id-ranges":{"1":[{"start-key":"00ff","end-key":"612062"}]}})
    );
}

#[tokio::test]
async fn task_078_existing_update_parse_failure() {
    let c = cluster();
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(78);
    let mut s = scheduler();
    s.prepare(&c).unwrap();
    s.update_grant_config(1, None, &c, &storage, &paths)
        .await
        .unwrap();
    let before = s.encode_grant_config().unwrap();
    assert!(matches!(
        s.update_grant_config(1, Some(&["%xx", "b"]), &c, &storage, &paths)
            .await,
        Err(GrantConfigError::Range(_))
    ));
    assert_eq!(s.encode_grant_config().unwrap(), before);
    assert_eq!(
        storage.load_scheduler_config(&paths, NAME).await.unwrap(),
        before
    );
    assert!(c.get_store(1).unwrap().allow_leader_transfer_out());
    s.prepare(&c).unwrap();
    s.update_grant_config(1, Some(&[]), &c, &storage, &paths)
        .await
        .unwrap();
    assert_eq!(
        s.config().stores()[&1],
        vec![astersql_pd_core::KeyRange::default()]
    );
    s.clean(&c);
    assert!(c.get_store(1).unwrap().allow_leader_transfer_out());
}

#[tokio::test]
async fn task_078_retained_ranges_reparse() {
    let c = cluster();
    let storage = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(78);
    for (key, expected) in [
        (b"a+b".to_vec(), Some(b"a b".to_vec())),
        (b"%41".to_vec(), Some(b"A".to_vec())),
        (vec![0, 255], Some(vec![0, 255])),
        (b"%xx".to_vec(), None),
    ] {
        let mut s =
            GrantLeaderScheduler::new(GrantLeaderConfig::new(std::collections::BTreeMap::from([
                (
                    1,
                    vec![astersql_pd_core::KeyRange {
                        start_key: key.clone(),
                        end_key: vec![],
                    }],
                ),
            ])));
        s.prepare(&c).unwrap();
        let result = s.update_grant_config(1, None, &c, &storage, &paths).await;
        if let Some(expected) = expected {
            result.unwrap();
            assert_eq!(s.config().stores()[&1][0].start_key, expected);
            s.clean(&c);
        } else {
            assert!(matches!(result, Err(GrantConfigError::Range(_))));
            assert_eq!(s.config().stores()[&1][0].start_key, key);
        }
        assert!(c.get_store(1).unwrap().allow_leader_transfer_out());
    }
    let mut s =
        GrantLeaderScheduler::new(GrantLeaderConfig::new(std::collections::BTreeMap::from([
            (1, vec![]),
        ])));
    s.prepare(&c).unwrap();
    s.update_grant_config(1, None, &c, &storage, &paths)
        .await
        .unwrap();
    assert_eq!(
        s.config().stores()[&1],
        vec![astersql_pd_core::KeyRange::default()]
    );
    s.clean(&c);
    println!("task_078: 5/5 retained-range reparse cases passed");
}
