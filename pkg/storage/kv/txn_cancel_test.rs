// Copyright 2026 AsterSQL.
use crate::{fixture, *};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

async fn snapshot(store: &dyn MetaStore) -> Snapshot {
    store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
}
async fn txn_cancel_contract(store: &dyn MetaStore) {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let before = snapshot(store).await;
    assert_eq!(
        RawTxn::new(store)
            .and_then(vec![Mutation::put(b"cancelled", b"bad")])
            .commit_with_cancel(&cancel)
            .await
            .unwrap_err(),
        StoreError::Cancelled
    );
    let after = snapshot(store).await;
    assert_eq!(after.entries, before.entries);
    assert_eq!(after.revision, before.revision);
    for ops in [
        vec![
            Mutation::put(b"partial", b"bad").into(),
            RawTxnOp::Get(vec![]),
        ],
        vec![
            Mutation::put(b"duplicate", b"one").into(),
            Mutation::put(b"duplicate", b"two").into(),
        ],
    ] {
        for selected in [true, false] {
            let txn = if selected {
                RawTxn::new(store).then_ops(ops.clone())
            } else {
                RawTxn::new(store).else_ops(ops.clone())
            };
            assert_eq!(
                txn.commit_with_cancel(&CancellationToken::new())
                    .await
                    .unwrap_err(),
                StoreError::InvalidArgument
            );
            let after = snapshot(store).await;
            assert_eq!(after.entries, before.entries);
            assert_eq!(after.revision, before.revision);
        }
    }
    let response = RawTxn::new(store)
        .when(vec![RawTxnCondition {
            key: b"missing".to_vec(),
            cmp_type: RawTxnCmpType::Exists,
            value: vec![],
        }])
        .and_then(vec![Mutation::put(b"unselected", b"bad")])
        .commit_with_cancel(&CancellationToken::new())
        .await
        .unwrap();
    assert!(!response.succeeded);
    assert_eq!(response.revision, before.revision);
    let response = RawTxn::new(store)
        .and_then(vec![Mutation::put(b"completed", b"ok")])
        .commit_with_cancel(&CancellationToken::new())
        .await
        .unwrap();
    assert!(response.succeeded);
    assert_eq!(response.revision, before.revision + 1);
}
#[tokio::test]
async fn txn_cancel_memory() {
    txn_cancel_contract(&MemoryStore::default()).await;
}

// Real TCP delivery boundary: requests reach etcd, armed responses are withheld.
struct Proxy {
    endpoint: String,
    armed: Arc<AtomicBool>,
    stop: CancellationToken,
    task: Option<JoinHandle<()>>,
}
impl Proxy {
    async fn start(upstream: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let upstream = upstream.strip_prefix("http://").unwrap().to_owned();
        let armed = Arc::new(AtomicBool::new(false));
        let stop = CancellationToken::new();
        let (gate, shutdown) = (armed.clone(), stop.clone());
        let task = tokio::spawn(async move {
            let work = async {
                let (downstream, _) = listener.accept().await.unwrap();
                let upstream = TcpStream::connect(upstream).await.unwrap();
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
                            std::future::pending::<()>().await;
                        }
                        if dw.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                };
                tokio::select! { _ = requests => {}, _ = responses => {} }
                // Keep the listener alive until shutdown; reconnects cannot reach etcd.
                std::future::pending::<()>().await;
            };
            tokio::select! { _ = shutdown.cancelled() => {}, _ = work => {} }
        });
        Self {
            endpoint,
            armed,
            stop,
            task: Some(task),
        }
    }
    async fn close(mut self) {
        self.stop.cancel();
        self.task.take().unwrap().await.unwrap();
    }
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
async fn uncertain_write(fixture: &fixture::Fixture, direct: &dyn MetaStore, cancel_request: bool) {
    let proxy = Proxy::start(&fixture.endpoint).await;
    let store = EtcdMetaStore::connect(
        std::slice::from_ref(&proxy.endpoint),
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    snapshot(&store).await; // Complete the HTTP/2 handshake before arming the response gate.
    let before = snapshot(direct).await;
    proxy.armed.store(true, Ordering::SeqCst);
    let key = if cancel_request {
        b"lost-cancel".as_slice()
    } else {
        b"lost-disconnect".as_slice()
    };
    let cancel = CancellationToken::new();
    let result = async {
        RawTxn::new(&store)
            .and_then(vec![Mutation::put(key, b"committed")])
            .commit_with_cancel(&cancel)
            .await
    };
    tokio::pin!(result);
    let observe_commit = async {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let after = snapshot(direct).await;
                if after.entries.iter().any(|e| e.key == key) {
                    return after;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("request must reach the real etcd commit point")
    };
    let committed = tokio::select! {
        result = &mut result => panic!("response escaped the armed proxy: {result:?}"),
        committed = observe_commit => committed,
    };
    assert_eq!(committed.revision, before.revision + 1);
    if cancel_request {
        cancel.cancel();
    }
    proxy.close().await; // Drop the TCP connection after the confirmed commit.
    let error = tokio::time::timeout(Duration::from_secs(4), &mut result)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(
        error,
        StoreError::Unavailable,
        "dispatched write has an unknown result"
    );
    let after = snapshot(direct).await;
    assert_eq!(after.entries, committed.entries);
    assert_eq!(
        after.revision, committed.revision,
        "no automatic transaction replay"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn txn_cancel_etcd() {
    let fixture = fixture::Fixture::start();
    let store = fixture.store().await;
    txn_cancel_contract(&store).await;
    uncertain_write(&fixture, &store, true).await;
    uncertain_write(&fixture, &store, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn txn_cancel_malformed_response() {
    let fixture = fixture::Fixture::start();
    let mut client = etcd_client::Client::connect([fixture.endpoint.as_str()], None)
        .await
        .unwrap();
    client.put("response-key", "value", None).await.unwrap();
    for succeeded in [true, false] {
        let response = client
            .txn(
                etcd_client::Txn::new()
                    .when(vec![etcd_client::Compare::create_revision(
                        "response-key",
                        etcd_client::CompareOp::Equal,
                        if succeeded { 2 } else { 0 },
                    )])
                    .and_then(vec![etcd_client::TxnOp::get("response-key", None)])
                    .or_else(vec![etcd_client::TxnOp::get("response-key", None)]),
            )
            .await
            .unwrap();
        assert_eq!(response.succeeded(), succeeded);
        let revision = response.header().unwrap().revision();
        let ops = vec![RawTxnOp::Get(b"response-key".to_vec())];
        let items = response.op_responses();
        let good = crate::etcd_store::decode_raw(succeeded, revision, &ops, items.clone()).unwrap();
        assert_eq!(good.responses[0].key_value_pairs[0].value, b"value");
        let put = client
            .txn(
                etcd_client::Txn::new().and_then(vec![etcd_client::TxnOp::put("other", "v", None)]),
            )
            .await
            .unwrap();
        for corrupt in [
            vec![],
            vec![items[0].clone(), items[0].clone()],
            put.op_responses(),
        ] {
            assert_eq!(
                crate::etcd_store::decode_raw(succeeded, revision, &ops, corrupt).unwrap_err(),
                StoreError::Backend
            );
        }
    }
}

#[test]
fn txn_cancel_error_categories() {
    use tonic::Code;
    for (code, expected) in [
        (Code::Cancelled, StoreError::Unavailable),
        (Code::DeadlineExceeded, StoreError::Unavailable),
        (Code::Unavailable, StoreError::Unavailable),
        (Code::Unknown, StoreError::Unavailable),
        (Code::InvalidArgument, StoreError::InvalidArgument),
        (Code::PermissionDenied, StoreError::PermissionDenied),
        (Code::Unauthenticated, StoreError::PermissionDenied),
        (Code::NotFound, StoreError::LeaseNotFound),
        (Code::Internal, StoreError::Backend),
    ] {
        let transport =
            etcd_client::Error::GRpcStatus(tonic::Status::new(code, "boundary failure"));
        let error = crate::etcd_store::map_error(transport);
        assert_eq!(
            crate::txn_cancel::dispatched_error(error),
            expected,
            "{code:?}"
        );
    }
}
