// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::{
    Base, CallbackTxnError, Consistency, MemoryStore, MetaStore, Read, StoreError,
};
use tokio_util::sync::CancellationToken;

async fn contract(store: &dyn MetaStore) {
    let base = Base::new(store);
    for count in [0, 1, 119, 120, 121, 240, 241] {
        let cancel = CancellationToken::new();
        let prefix = format!("/success/{count}/");
        let before = store
            .read(Read::prefix(prefix.as_bytes(), Consistency::Linearizable))
            .await
            .unwrap();
        let mut seen = vec![];
        run_batch_op_in_txn(
            store,
            &cancel,
            &(0..count).collect::<Vec<_>>(),
            async |i, txn| {
                seen.push(*i);
                let key = format!("{prefix}{i:04}");
                txn.save(key.as_bytes(), b"value").unwrap();
                Ok::<_, &'static str>(())
            },
        )
        .await
        .unwrap();
        assert_eq!(seen, (0..count).collect::<Vec<_>>());
        let after = store
            .read(Read::prefix(prefix.as_bytes(), Consistency::Linearizable))
            .await
            .unwrap();
        assert_eq!(after.entries.len(), count);
        assert_eq!(after.revision - before.revision, count.div_ceil(120) as i64);
        for (i, entry) in after.entries.iter().enumerate() {
            assert_eq!(entry.mod_revision - before.revision, (i / 120 + 1) as i64);
            assert_eq!(entry.value, b"value");
        }
    }
    for fail_at in [0, 119, 120, 121, 240] {
        let prefix = format!("/failed/{fail_at}/");
        let mut seen = vec![];
        let result = run_batch_op_in_txn(
            store,
            &CancellationToken::new(),
            &(0..242).collect::<Vec<_>>(),
            async |i, txn| {
                seen.push(*i);
                txn.save(format!("{prefix}{i:04}").as_bytes(), b"buffered")
                    .unwrap();
                if *i == fail_at {
                    Err("sentinel")
                } else {
                    Ok(())
                }
            },
        )
        .await;
        assert_eq!(result, Err(CallbackTxnError::Callback("sentinel")));
        assert_eq!(seen, (0..=fail_at).collect::<Vec<_>>());
        let entries = store
            .read(Read::prefix(prefix.as_bytes(), Consistency::Linearizable))
            .await
            .unwrap()
            .entries;
        assert_eq!(entries.len(), fail_at / 120 * 120);
    }
    // Cancel during the second chunk: its callbacks finish, but it never commits.
    let cancel = CancellationToken::new();
    let mut seen = vec![];
    let result = run_batch_op_in_txn(
        store,
        &cancel,
        &(0..241).collect::<Vec<_>>(),
        async |i, txn| {
            seen.push(*i);
            txn.save(format!("/cancel/{i:04}").as_bytes(), b"buffered")
                .unwrap();
            if *i == 120 {
                cancel.cancel();
            }
            Ok::<_, &'static str>(())
        },
    )
    .await;
    assert_eq!(result, Err(CallbackTxnError::Store(StoreError::Cancelled)));
    assert_eq!(seen.len(), 240);
    let entries = store
        .read(Read::prefix(b"/cancel/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    assert_eq!(entries.len(), 120);
    // Pre-cancelled contexts still invoke callbacks; callback errors take priority.
    let mut seen = 0;
    let result = run_batch_op_in_txn(store, &cancel, &[0], async |_, txn| {
        seen += 1;
        txn.save(b"/pre-cancel", b"not committed").unwrap();
        Ok::<_, &'static str>(())
    })
    .await;
    assert_eq!(seen, 1);
    assert_eq!(result, Err(CallbackTxnError::Store(StoreError::Cancelled)));
    assert!(base.load(b"/pre-cancel").await.unwrap().is_empty());
    assert_eq!(
        run_batch_op_in_txn(store, &cancel, &[0], async |_, _| Err("priority")).await,
        Err(CallbackTxnError::Callback("priority"))
    );
    run_batch_op_in_txn(store, &cancel, &[] as &[usize], async |_, _| Err("never"))
        .await
        .unwrap();
    // The first committed chunk survives a real read-guard conflict in chunk two.
    base.save(b"/guard", b"old").await.unwrap();
    let mut seen = 0;
    let result = run_batch_op_in_txn(
        store,
        &CancellationToken::new(),
        &(0..241).collect::<Vec<_>>(),
        async |i, txn| {
            seen += 1;
            txn.save(format!("/conflict/{i:04}").as_bytes(), b"value")?;
            if *i == 120 {
                assert_eq!(txn.load(b"/guard").await?, b"old");
                base.save(b"/guard", b"concurrent").await?;
            }
            Ok::<_, StoreError>(())
        },
    )
    .await;
    assert_eq!(result, Err(CallbackTxnError::Conflict));
    assert_eq!(seen, 240);
    assert_eq!(
        store
            .read(Read::prefix(b"/conflict/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .len(),
        120
    );
}

#[tokio::test]
async fn batch_ops_memory() {
    contract(&MemoryStore::default()).await;
}

#[tokio::test]
async fn batch_ops_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(&fixture.store().await).await;
    let store = fixture.store().await;
    let mut seen = 0;
    let began = std::time::Instant::now();
    let result = run_batch_op_in_txn(
        &store,
        &CancellationToken::new(),
        &(0..241).collect::<Vec<_>>(),
        async |i, txn| {
            seen += 1;
            txn.save(format!("/disconnect/{i:04}").as_bytes(), b"value")?;
            if *i == 120 {
                fixture.command("stop", "stopped");
            }
            Ok::<_, StoreError>(())
        },
    )
    .await;
    assert!(matches!(result, Err(CallbackTxnError::Store(_))));
    assert_eq!(seen, 240);
    fixture.command("start", "restarted");
    let entries = store
        .read(Read::prefix(b"/disconnect/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    assert_eq!(entries.len(), 120);
    assert!(entries.iter().all(|entry| entry.value == b"value"));
    eprintln!(
        "batch_ops: first chunk committed -> stop acknowledged during callback 120 -> second commit fails -> restart same WAL -> exactly 120 keys, elapsed {:?}",
        began.elapsed()
    );
}

#[test]
fn batch_ops_limit() {
    assert_eq!(MAX_BATCH_OPS, 120);
}

#[tokio::test]
async fn batch_ops_inflight_cancel_etcd() {
    use astersql_pd_storage_kv::EtcdMetaStore;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let fixture = endpoint_codec_test::fixture::Fixture::start();
    let direct = fixture.store().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let upstream = fixture.endpoint.strip_prefix("http://").unwrap().to_owned();
    let armed = Arc::new(AtomicBool::new(false));
    let held = Arc::new(tokio::sync::Notify::new());
    let (gate, signal) = (armed.clone(), held.clone());
    let proxy = tokio::spawn(async move {
        let (downstream, _) = listener.accept().await.unwrap();
        let upstream = tokio::net::TcpStream::connect(upstream).await.unwrap();
        let (mut dr, mut dw) = downstream.into_split();
        let (mut ur, mut uw) = upstream.into_split();
        let responses = async {
            let mut bytes = [0; 65536];
            loop {
                let n = ur.read(&mut bytes).await.unwrap();
                if n == 0 {
                    break;
                }
                if gate.load(Ordering::SeqCst) {
                    signal.notify_one();
                    std::future::pending::<()>().await;
                }
                dw.write_all(&bytes[..n]).await.unwrap();
            }
        };
        tokio::select! { _ = tokio::io::copy(&mut dr, &mut uw) => {}, _ = responses => {} }
    });
    // Abort on unwind as well as normal completion; proxy only controls external IO.
    struct ProxyTask(Option<tokio::task::JoinHandle<()>>);
    impl Drop for ProxyTask {
        fn drop(&mut self) {
            if let Some(task) = &self.0 {
                task.abort();
            }
        }
    }
    let mut proxy = ProxyTask(Some(proxy));
    let store = EtcdMetaStore::connect(&[endpoint], Duration::from_secs(2))
        .await
        .unwrap();
    Base::new(&store).save(b"/warmup", b"ready").await.unwrap();
    armed.store(true, Ordering::SeqCst);
    let cancel = CancellationToken::new();
    let mut seen = 0;
    let items = (0..121).collect::<Vec<_>>();
    {
        let result = run_batch_op_in_txn(&store, &cancel, &items, async |i, txn| {
            seen += 1;
            txn.save(format!("/inflight/{i:04}").as_bytes(), b"value")
        });
        tokio::pin!(result);
        tokio::select! {
            result = &mut result => panic!("commit escaped response gate: {result:?}"),
            held = tokio::time::timeout(Duration::from_secs(3), held.notified()) => { held.unwrap(); }
        }
        // Before cancelling, direct linearizable reads prove etcd committed chunk one.
        assert_eq!(
            direct
                .read(Read::prefix(b"/inflight/", Consistency::Linearizable))
                .await
                .unwrap()
                .entries
                .len(),
            120
        );
        let began = std::time::Instant::now();
        cancel.cancel();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), &mut result)
                .await
                .unwrap(),
            Err(CallbackTxnError::Store(StoreError::Unavailable))
        );
        eprintln!(
            "batch_ops: real commit confirmed -> cancel returns Unavailable after {:?}",
            began.elapsed()
        );
    }
    assert_eq!(seen, 120);
    let task = proxy.0.take().unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    // The next chunk was never invoked; cancellation cannot roll back chunk one.
    assert_eq!(
        direct
            .read(Read::prefix(b"/inflight/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .len(),
        120
    );
}
