// Copyright 2026 AsterSQL.

use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_kv::{Base, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(33);
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let cancel = CancellationToken::new();
    // Actual Go TestDataPhysicalRepresentation bytes, independently checked by Go.
    for (id, txn, gc) in [
        (NULL_KEYSPACE_ID, "456139133457530882", "654882009e40000"),
        (
            1111,
            "456139133457530883",
            r#"{"keyspace_id":1111,"safe_point":456139133457530881}"#,
        ),
        (0, "", ""),
        (15, "18446744073709551616", r#"{"safe_point":71}"#),
        (11, "invalid", r#"{"safe_point":71}"#),
        (12, "144", r#"{"safe_point":"#),
        (
            14,
            "18446744073709551615",
            r#"{"safe_point":18446744073709551615}"#,
        ),
        (7, "126", r#"{"keyspace_id":999,"safe_point":63}"#),
    ] {
        base.save(paths.txn_safe_point_path(id).as_bytes(), txn.as_bytes())
            .await
            .unwrap();
        base.save(paths.gc_safe_point_path(id).as_bytes(), gc.as_bytes())
            .await
            .unwrap();
    }
    let before = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    let ids = [11, NULL_KEYSPACE_ID, 9, 7, 12, 0, 14, 1111, 15];
    let results = ep
        .load_gc_safe_point_pairs(&paths, &ids, &cancel)
        .await
        .unwrap();
    assert_eq!(results.len(), ids.len());
    for (row, id) in results.iter().zip(ids) {
        assert_eq!(row.keyspace_id, id);
        if id == 11 {
            assert!(matches!(row.err, Some(SafePointReadError::ParseUint)));
            assert_eq!((row.txn_safe_point, row.gc_safe_point), (0, 0));
        } else if id == 15 {
            // Go decodeTxnSafePoint returns ParseUint's saturated value and error.
            assert!(matches!(row.err, Some(SafePointReadError::ParseUint)));
            assert_eq!((row.txn_safe_point, row.gc_safe_point), (u64::MAX, 0));
        } else if id == 12 {
            assert!(matches!(
                row.err,
                Some(SafePointReadError::JsonUnmarshal(_))
            ));
            assert_eq!((row.txn_safe_point, row.gc_safe_point), (144, 0));
        } else {
            assert!(row.err.is_none());
            assert_eq!(
                row.txn_safe_point,
                ep.load_txn_safe_point(&paths, id).await.unwrap()
            );
            assert_eq!(
                row.gc_safe_point,
                ep.load_gc_safe_point(&paths, id).await.unwrap()
            );
        }
    }
    assert_eq!(results[1].gc_safe_point, 456139133457530880);
    assert_eq!(results[7].gc_safe_point, 456139133457530881);
    assert_eq!(results[6].txn_safe_point, u64::MAX);
    assert_eq!(results[6].gc_safe_point, u64::MAX);
    let after = store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);
    assert_eq!(before.revision, after.revision);
    for raw in [b"not-a-safe-point".as_slice(), b"18446744073709551616", b""] {
        base.save(paths.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes(), raw)
            .await
            .unwrap();
        let rows = ep
            .load_gc_safe_point_pairs(&paths, &[NULL_KEYSPACE_ID, 7], &cancel)
            .await
            .unwrap();
        assert_eq!(rows[0].err.is_some(), !raw.is_empty());
        assert!(rows[1].err.is_none());
        assert_eq!(rows[1].gc_safe_point, 63);
    }
    base.save(
        paths.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes(),
        b"654882009e40000",
    )
    .await
    .unwrap();
    assert!(
        ep.load_gc_safe_point_pairs(&paths, &[], &cancel)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        ep.load_gc_safe_point_pairs(&paths, &[7, 7], &cancel).await,
        Err(GCSafePointBatchError::Duplicate(7))
    ));
    assert!(matches!(
        ep.load_gc_safe_point_pairs(&paths, &[7; 61], &cancel).await,
        Err(GCSafePointBatchError::TooLarge)
    ));
    let ids: Vec<u32> = (0..60).map(|i| 10000 + i * 101).collect();
    for (i, id) in ids.iter().enumerate() {
        base.save(
            paths.txn_safe_point_path(*id).as_bytes(),
            (100 + i).to_string().as_bytes(),
        )
        .await
        .unwrap();
        base.save(
            paths.gc_safe_point_path(*id).as_bytes(),
            format!(r#"{{"safe_point":{}}}"#, 50 + i).as_bytes(),
        )
        .await
        .unwrap();
    }
    let rows = ep
        .load_gc_safe_point_pairs(&paths, &ids, &cancel)
        .await
        .unwrap();
    assert_eq!(rows.len(), 60);
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row.keyspace_id, ids[i]);
        assert_eq!(row.txn_safe_point, (100 + i) as u64);
        assert_eq!(row.gc_safe_point, (50 + i) as u64);
        assert!(row.err.is_none());
    }
    // A writer updates both fields and both scopes in a single real transaction.
    // Batch readers may observe any committed generation, never a mixed snapshot.
    let race_paths = ClusterPaths::new(330);
    let writer = StorageEndpoint::new(store.clone());
    let writer_paths = race_paths.clone();
    let writing = async {
        for generation in 1..=40 {
            writer
                .run_in_gc_state_transaction(&writer_paths, async |batch| {
                    for id in [1, 2] {
                        batch.set_txn_safe_point(id, generation);
                        batch.set_gc_safe_point(id, generation);
                    }
                    Ok::<_, ()>(())
                })
                .await
                .unwrap();
            tokio::task::yield_now().await;
        }
    };
    let reading = async {
        for _ in 0..80 {
            let rows = ep
                .load_gc_safe_point_pairs(&race_paths, &[2, 1], &cancel)
                .await
                .unwrap();
            assert_eq!(rows.len(), 2);
            assert!(rows.iter().all(|row| row.err.is_none()));
            let generation = rows[0].txn_safe_point;
            assert!(
                rows.iter()
                    .all(|row| row.txn_safe_point == generation && row.gc_safe_point == generation)
            );
            tokio::task::yield_now().await;
        }
    };
    tokio::join!(writing, reading);
    let final_rows = ep
        .load_gc_safe_point_pairs(&race_paths, &[2, 1], &cancel)
        .await
        .unwrap();
    assert!(
        final_rows
            .iter()
            .all(|row| row.txn_safe_point == 40 && row.gc_safe_point == 40)
    );
    cancel.cancel();
    assert!(matches!(
        ep.load_gc_safe_point_pairs(&paths, &[1], &cancel).await,
        Err(GCSafePointBatchError::Store(
            astersql_pd_storage_kv::StoreError::Cancelled
        ))
    ));
    assert!(
        ep.load_gc_safe_point_pairs(&paths, &[], &cancel)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn gc_batch_read_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}

#[tokio::test]
async fn gc_batch_read_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let paths = ClusterPaths::new(34);
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    let cancel = CancellationToken::new();
    ep.run_in_gc_state_transaction(&paths, async |batch| {
        batch.set_gc_safe_point(1, 42);
        batch.set_txn_safe_point(1, 84);
        Ok::<_, ()>(())
    })
    .await
    .unwrap();
    fixture.command("stop", "stopped");
    let began = std::time::Instant::now();
    assert!(matches!(
        ep.load_gc_safe_point_pairs(&paths, &[1], &cancel).await,
        Err(GCSafePointBatchError::Store(_))
    ));
    assert!(
        ep.load_gc_safe_point_pairs(&paths, &[], &cancel)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        ep.load_gc_safe_point_pairs(&paths, &[1, 1], &cancel).await,
        Err(GCSafePointBatchError::Duplicate(1))
    ));
    fixture.command("start", "restarted");
    let rows = ep
        .load_gc_safe_point_pairs(&paths, &[1], &cancel)
        .await
        .unwrap();
    assert_eq!((rows[0].txn_safe_point, rows[0].gc_safe_point), (84, 42));
    assert!(rows[0].err.is_none());
    eprintln!(
        "gc_batch_read: stop acknowledged -> whole-batch Store error -> same WAL restart -> (84,42), elapsed {:?}",
        began.elapsed()
    );
}

#[test]
fn gc_batch_read_response_validation() {
    use astersql_pd_storage_kv::{KeyValuePair, RawTxnResponse, RawTxnResponseItem};
    let ids = [1];
    let keys = vec![b"txn".to_vec(), b"gc".to_vec()];
    let good = RawTxnResponse {
        succeeded: true,
        revision: 1,
        responses: vec![
            RawTxnResponseItem {
                key_value_pairs: vec![KeyValuePair {
                    key: keys[0].clone(),
                    value: b"invalid".to_vec(),
                }],
            },
            RawTxnResponseItem::default(),
        ],
    };
    // Pure transport validation, not a fake backend returning transaction results.
    let mut invalid = good.clone();
    invalid.succeeded = false;
    assert!(matches!(
        gc_batch_read::decode_response(&ids, &keys, invalid),
        Err(GCSafePointBatchError::InvalidResponse)
    ));
    for count in [1, 3] {
        let mut invalid = good.clone();
        invalid
            .responses
            .resize(count, RawTxnResponseItem::default());
        assert!(matches!(
            gc_batch_read::decode_response(&ids, &keys, invalid),
            Err(GCSafePointBatchError::InvalidResponse)
        ));
    }
    for multiple in [false, true] {
        let mut invalid = good.clone();
        invalid.responses[1].key_value_pairs = vec![
            KeyValuePair {
                key: b"wrong-key".to_vec(),
                value: vec![]
            };
            if multiple { 2 } else { 1 }
        ];
        assert!(
            matches!(gc_batch_read::decode_response(&ids, &keys, invalid), Err(GCSafePointBatchError::InvalidRead(key)) if key == keys[1])
        );
    }
    let rows = gc_batch_read::decode_response(&ids, &keys, good).unwrap();
    assert!(matches!(rows[0].err, Some(SafePointReadError::ParseUint)));
}

// A real TCP response gate exercises cancellation after dispatch without replacing
// transactions or their results. Requests always reach the real etcd backend.
struct ResponseGate {
    endpoint: String,
    armed: Arc<std::sync::atomic::AtomicBool>,
    held: Arc<tokio::sync::Notify>,
    stop: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl ResponseGate {
    async fn start(upstream: &str) -> Self {
        use std::sync::atomic::Ordering;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let upstream = upstream.strip_prefix("http://").unwrap().to_owned();
        let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let held = Arc::new(tokio::sync::Notify::new());
        let stop = CancellationToken::new();
        let (gate, signal, shutdown) = (armed.clone(), held.clone(), stop.clone());
        let task = tokio::spawn(async move {
            let work = async {
                let (downstream, _) = listener.accept().await.unwrap();
                let upstream = tokio::net::TcpStream::connect(upstream).await.unwrap();
                let (mut dr, mut dw) = downstream.into_split();
                let (mut ur, mut uw) = upstream.into_split();
                let requests = tokio::io::copy(&mut dr, &mut uw);
                let responses = async {
                    let mut buf = [0; 65536];
                    loop {
                        let n = ur.read(&mut buf).await.unwrap();
                        if n == 0 {
                            break;
                        }
                        if gate.load(Ordering::SeqCst) {
                            signal.notify_one();
                            std::future::pending::<()>().await;
                        }
                        if dw.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                };
                tokio::select! { _ = requests => {}, _ = responses => {} }
                std::future::pending::<()>().await;
            };
            tokio::select! { _ = shutdown.cancelled() => {}, _ = work => {} }
        });
        Self {
            endpoint,
            armed,
            held,
            stop,
            task: Some(task),
        }
    }
    async fn close(mut self) {
        self.stop.cancel();
        self.task.take().unwrap().await.unwrap();
    }
}
impl Drop for ResponseGate {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

#[tokio::test]
async fn gc_batch_read_inflight_cancel_etcd() {
    use astersql_pd_storage_kv::{EtcdMetaStore, StoreError};
    use std::{sync::atomic::Ordering, time::Duration};
    let fixture = endpoint_codec_test::fixture::Fixture::start();
    let direct = fixture.store().await;
    let paths = ClusterPaths::new(35);
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    ep.run_in_gc_state_transaction(&paths, async |batch| {
        batch.set_gc_safe_point(1, 42);
        batch.set_txn_safe_point(1, 84);
        Ok::<_, ()>(())
    })
    .await
    .unwrap();
    let before = direct
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    let proxy = ResponseGate::start(&fixture.endpoint).await;
    let store = EtcdMetaStore::connect(
        std::slice::from_ref(&proxy.endpoint),
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    let endpoint = StorageEndpoint::new(Arc::new(store));
    let cancel = CancellationToken::new();
    let rows = endpoint
        .load_gc_safe_point_pairs(&paths, &[1], &cancel)
        .await
        .unwrap();
    assert_eq!((rows[0].txn_safe_point, rows[0].gc_safe_point), (84, 42));
    proxy.armed.store(true, Ordering::SeqCst);
    let result = endpoint.load_gc_safe_point_pairs(&paths, &[1], &cancel);
    tokio::pin!(result);
    tokio::select! {
        result = &mut result => panic!("read escaped response gate: {result:?}"),
        held = tokio::time::timeout(Duration::from_secs(3), proxy.held.notified()) => { held.expect("real backend response reaches gate"); },
    }
    let began = std::time::Instant::now();
    cancel.cancel();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), &mut result)
            .await
            .unwrap(),
        Err(GCSafePointBatchError::Store(StoreError::Cancelled))
    ));
    proxy.close().await;
    let after = direct
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);
    assert_eq!(before.revision, after.revision);
    eprintln!(
        "gc_batch_read: real etcd response held -> cancel -> Cancelled, state unchanged, elapsed {:?}",
        began.elapsed()
    );
}
