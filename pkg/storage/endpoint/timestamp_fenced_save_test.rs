// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, Compare, MemoryStore, MetaStore, StoreError};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

// Actual Go TestUint64ToBytes output preserved in theme3 task 3.
const GO_1000: &[u8] = b"\x00\x00\x00\x00\x00\x00\x03\xe8";
fn time(n: i64) -> SystemTime {
    if n >= 0 {
        UNIX_EPOCH + Duration::from_nanos(n as u64)
    } else {
        UNIX_EPOCH - Duration::from_nanos(n.unsigned_abs())
    }
}
fn term() -> Vec<Compare> {
    vec![Compare::Value(b"leader".to_vec(), b"term-1".to_vec())]
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(42);
    let cancel = CancellationToken::new();
    base.save(b"leader", b"term-1").await.unwrap();
    for group in [0, 1, u32::MAX] {
        ep.save_timestamp(&paths, group, time(1000), term(), &cancel)
            .await
            .unwrap();
        assert_eq!(
            base.load(paths.timestamp_path(group).as_bytes())
                .await
                .unwrap(),
            GO_1000
        );
        for n in [1000, 999, -1] {
            assert!(matches!(
                ep.save_timestamp(&paths, group, time(n), term(), &cancel)
                    .await,
                Err(TimestampSaveError::NotIncreasing)
            ));
            assert_eq!(
                ep.load_timestamp(&paths, group).await.unwrap(),
                Some(time(1000))
            );
        }
        ep.save_timestamp(&paths, group, time(1001), term(), &cancel)
            .await
            .unwrap();
        assert_eq!(
            ep.load_timestamp(&paths, group).await.unwrap(),
            Some(time(1001))
        );
    }
    assert!(matches!(
        ep.save_timestamp(&paths, 0, time(2000), vec![], &cancel)
            .await,
        Err(TimestampSaveError::NotLeader)
    ));
    assert!(matches!(
        ep.save_timestamp(
            &paths,
            0,
            time(2000),
            vec![Compare::Value(b"leader".to_vec(), vec![])],
            &cancel
        )
        .await,
        Err(TimestampSaveError::NotLeader)
    ));
    for value in [b"term-2".as_slice(), b""] {
        base.save(b"leader", value).await.unwrap();
        assert!(matches!(
            ep.save_timestamp(&paths, 0, time(2000), term(), &cancel)
                .await,
            Err(TimestampSaveError::Conflict)
        ));
        assert_eq!(
            ep.load_timestamp(&paths, 0).await.unwrap(),
            Some(time(1001))
        );
    }
    base.remove(b"leader").await.unwrap();
    assert!(matches!(
        ep.save_timestamp(&paths, 0, time(2000), term(), &cancel)
            .await,
        Err(TimestampSaveError::Conflict)
    ));
    base.save(b"leader", b"term-1").await.unwrap();
    let key = paths.timestamp_path(10);
    base.save(key.as_bytes(), b"bad").await.unwrap();
    assert!(matches!(
        ep.save_timestamp(&paths, 10, time(1000), term(), &cancel)
            .await,
        Err(TimestampSaveError::Decode(_))
    ));
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), b"bad");
    base.save(key.as_bytes(), b"").await.unwrap();
    ep.save_timestamp(&paths, 10, time(-1), term(), &cancel)
        .await
        .unwrap();
    ep.save_timestamp(&paths, 10, time(0), term(), &cancel)
        .await
        .unwrap();
    assert!(matches!(
        ep.save_timestamp(&paths, 10, time(0), term(), &cancel)
            .await,
        Err(TimestampSaveError::NotIncreasing)
    ));
    let stopped = CancellationToken::new();
    stopped.cancel();
    assert!(matches!(
        ep.save_timestamp(&paths, 0, time(2000), term(), &stopped)
            .await,
        Err(TimestampSaveError::Store(StoreError::Cancelled))
    ));
    assert_eq!(
        ep.load_timestamp(&paths, 0).await.unwrap(),
        Some(time(1001))
    );
    // A second condition must participate in the same real transaction.
    let mut fence = term();
    fence.push(Compare::Exists(b"missing-fence".to_vec()));
    assert!(matches!(
        ep.save_timestamp(&paths, 0, time(2000), fence, &cancel)
            .await,
        Err(TimestampSaveError::Conflict)
    ));
    assert_eq!(
        ep.load_timestamp(&paths, 0).await.unwrap(),
        Some(time(1001))
    );
}
#[tokio::test]
async fn timestamp_fenced_save_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn timestamp_fenced_save_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    let paths = ClusterPaths::new(42);
    assert_eq!(
        ep.load_timestamp(&paths, 0).await.unwrap(),
        Some(time(1001))
    );
    drop(fixture);
    assert!(matches!(
        ep.save_timestamp(&paths, 0, time(2000), term(), &CancellationToken::new())
            .await,
        Err(TimestampSaveError::Store(_))
    ));
}
#[test]
fn timestamp_fenced_save_go_bytes() {
    assert_eq!(astersql_pd_typeutil::u64_to_bytes(1000), GO_1000);
    assert_eq!(
        astersql_pd_typeutil::parse_timestamp(GO_1000).unwrap(),
        time(1000)
    );
}

// Pause only the IO dispatch boundary; all reads and commits use the real backend.
struct DispatchGate {
    store: Arc<dyn MetaStore>,
    entered: tokio::sync::Semaphore,
    release: tokio::sync::Semaphore,
}
impl DispatchGate {
    async fn wait(&self, count: u32) {
        tokio::time::timeout(Duration::from_secs(10), self.entered.acquire_many(count))
            .await
            .unwrap()
            .unwrap()
            .forget();
    }
}
#[async_trait::async_trait]
impl MetaStore for DispatchGate {
    async fn read(
        &self,
        r: astersql_pd_storage_kv::Read,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Snapshot> {
        self.store.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<astersql_pd_storage_kv::Mutation>,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Commit> {
        self.entered.add_permits(1);
        self.release.acquire().await.unwrap().forget();
        self.store.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<astersql_pd_storage_kv::Mutation>,
        e: Vec<astersql_pd_storage_kv::Mutation>,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Commit> {
        self.store.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<astersql_pd_storage_kv::RawTxnOp>,
        e: Vec<astersql_pd_storage_kv::RawTxnOp>,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::RawTxnResponse> {
        self.store.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: i64,
    ) -> astersql_pd_storage_kv::Result<Box<dyn astersql_pd_storage_kv::MetaWatch>> {
        self.store.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Lease> {
        self.store.grant(t).await
    }
    async fn keep_alive(
        &self,
        l: i64,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Lease> {
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
async fn races(store: Arc<dyn MetaStore>) {
    let gate = Arc::new(DispatchGate {
        store: store.clone(),
        entered: tokio::sync::Semaphore::new(0),
        release: tokio::sync::Semaphore::new(0),
    });
    let ep = StorageEndpoint::new(gate.clone());
    let paths = ClusterPaths::new(44);
    let cancel = CancellationToken::new();
    let base = Base::new(store.as_ref());
    base.save(b"leader", b"term-1").await.unwrap();
    base.save(paths.timestamp_path(0).as_bytes(), GO_1000)
        .await
        .unwrap();
    for group in [0, 1, 2] {
        if group == 2 {
            base.save(paths.timestamp_path(group).as_bytes(), b"")
                .await
                .unwrap();
        }
        // Cover an existing value, absence (create revision guard), and present empty bytes.
        let first = ep.save_timestamp(&paths, group, time(2000), term(), &cancel);
        let second = ep.save_timestamp(&paths, group, time(2000), term(), &cancel);
        let control = async {
            gate.wait(2).await;
            gate.release.add_permits(2);
        };
        let (a, b, ()) = tokio::join!(first, second, control);
        assert!(matches!(
            (&a, &b),
            (Ok(()), Err(TimestampSaveError::Conflict))
                | (Err(TimestampSaveError::Conflict), Ok(()))
        ));
        assert_eq!(
            ep.load_timestamp(&paths, group).await.unwrap(),
            Some(time(2000))
        );
    }
    // Leadership changes after the timestamp read, before atomic commit.
    let save = ep.save_timestamp(&paths, 0, time(3000), term(), &cancel);
    let control = async {
        gate.wait(1).await;
        base.save(b"leader", b"term-2").await.unwrap();
        gate.release.add_permits(1);
    };
    let (result, ()) = tokio::join!(save, control);
    assert!(matches!(result, Err(TimestampSaveError::Conflict)));
    assert_eq!(
        ep.load_timestamp(&paths, 0).await.unwrap(),
        Some(time(2000))
    );
    // A lease-bound term disappears after reading but before commit.
    let lease = store.grant(60).await.unwrap();
    store
        .transact(
            vec![],
            vec![astersql_pd_storage_kv::Mutation::Put {
                key: b"leased-leader".to_vec(),
                value: b"term-lease".to_vec(),
                lease: Some(lease.id),
            }],
        )
        .await
        .unwrap();
    let fence = vec![
        Compare::Value(b"leased-leader".to_vec(), b"term-lease".to_vec()),
        Compare::Lease(b"leased-leader".to_vec(), lease.id),
    ];
    let save = ep.save_timestamp(&paths, 0, time(3000), fence, &cancel);
    let control = async {
        gate.wait(1).await;
        store.revoke(lease.id).await.unwrap();
        gate.release.add_permits(1);
    };
    let (result, ()) = tokio::join!(save, control);
    assert!(matches!(result, Err(TimestampSaveError::Conflict)));
    assert_eq!(
        ep.load_timestamp(&paths, 0).await.unwrap(),
        Some(time(2000))
    );
    base.save(b"leader", b"term-1").await.unwrap();
    let save = ep.save_timestamp(&paths, 0, time(3000), term(), &cancel);
    let control = async {
        gate.wait(1).await;
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(save, control);
    assert!(matches!(
        result,
        Err(TimestampSaveError::Store(StoreError::Unavailable))
    ));
    assert_eq!(
        ep.load_timestamp(&paths, 0).await.unwrap(),
        Some(time(2000))
    );
}
#[tokio::test]
async fn timestamp_fenced_save_memory_races() {
    races(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn timestamp_fenced_save_etcd_races() {
    let fixture = endpoint_codec_test::fixture::Fixture::start();
    races(Arc::new(fixture.store().await)).await;
}
