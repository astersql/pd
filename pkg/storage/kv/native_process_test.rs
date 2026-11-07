// Copyright 2026 AsterSQL.
//! Loopback-only subprocess fixture. Admin and data routes are private to the test driver.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    routing::post,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use openraft::BasicNode;
use openraft::raft::{AppendEntriesRequest, InstallSnapshotRequest, VoteRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

use crate::candidate_bench_test::{Loopback, listen_address};
use crate::native_open_test::config;
use crate::*;

struct Node {
    store: NativeMetaStore,
    isolated: Arc<AtomicBool>,
}

impl Node {
    fn accept_peer(&self) -> std::result::Result<(), StatusCode> {
        if self.isolated.load(Ordering::Acquire) {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct Request {
    prefix: String,
}

#[derive(Deserialize)]
struct Write {
    puts: Vec<(String, String)>,
}

async fn status(State(node): State<Arc<Node>>) -> Json<Value> {
    let metrics = node.store.consensus().metrics().borrow().clone();
    Json(json!({"leader": metrics.current_leader, "node": metrics.id,
        "role": format!("{:?}", metrics.state), "term": metrics.current_term,
        "applied": metrics.last_applied.map(|id| id.index)}))
}

async fn write(State(node): State<Arc<Node>>, Json(req): Json<Write>) -> Json<Value> {
    let writes: std::result::Result<Vec<_>, _> = req
        .puts
        .into_iter()
        .map(|(k, v)| Ok(Mutation::put(&STANDARD.decode(k)?, &STANDARD.decode(v)?)))
        .collect::<std::result::Result<_, base64::DecodeError>>();
    let Ok(writes) = writes else {
        return Json(json!({"error": "invalid encoding"}));
    };
    match node.store.transact(Vec::new(), writes).await {
        Ok(reply) => Json(json!({"revision": reply.revision})),
        Err(error) => Json(json!({"error": error})),
    }
}

async fn read(State(node): State<Arc<Node>>, Json(req): Json<Request>) -> Json<Value> {
    let Ok(prefix) = STANDARD.decode(req.prefix) else {
        return Json(json!({"error": "invalid encoding"}));
    };
    match node
        .store
        .read(Read::prefix(&prefix, Consistency::Linearizable))
        .await
    {
        Ok(reply) => Json(json!({"revision": reply.revision, "entries": reply.entries})),
        Err(error) => Json(json!({"error": error})),
    }
}

async fn compact(State(node): State<Arc<Node>>, Json(revision): Json<Revision>) -> Json<Value> {
    match node.store.compact(revision).await {
        Ok(()) => Json(json!({"compacted": revision})),
        Err(error) => Json(json!({"error": error})),
    }
}

#[derive(Deserialize)]
struct BenchPoint {
    key: String,
}

#[derive(Deserialize)]
struct BenchPut {
    key: String,
    value: String,
}

#[derive(Deserialize)]
struct BenchOp {
    request_put: BenchPut,
}

#[derive(Deserialize)]
struct BenchTxn {
    success: [BenchOp; 2],
}

async fn bench_status(State(node): State<Arc<Node>>) -> Json<Value> {
    let raft = node.store.consensus();
    let id = raft.metrics().borrow().id;
    Json(
        json!({"leader": raft.current_leader().await.map(|id| id.to_string()),
                "header": {"member_id": id.to_string()}}),
    )
}

async fn bench_write(state: State<Arc<Node>>, Json(req): Json<BenchTxn>) -> Json<Value> {
    let response = write(
        state,
        Json(Write {
            puts: req
                .success
                .into_iter()
                .map(|op| (op.request_put.key, op.request_put.value))
                .collect(),
        }),
    )
    .await;
    if let Some(revision) = response.0.get("revision") {
        return Json(json!({"succeeded": true, "header": {"revision": revision}}));
    }
    response
}

async fn bench_read(State(node): State<Arc<Node>>, Json(req): Json<BenchPoint>) -> Json<Value> {
    let Ok(key) = STANDARD.decode(req.key) else {
        return Json(json!({"error": "invalid encoding"}));
    };
    match node
        .store
        .read(Read::range(&key, b"", Consistency::Linearizable))
        .await
    {
        Ok(reply) => Json(json!({"kvs": reply.entries.into_iter()
            .map(|entry| json!({"value": STANDARD.encode(entry.value)})).collect::<Vec<_>>()})),
        Err(error) => Json(json!({"error": error})),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "loopback subprocess entry for native recovery, history and soak drivers"]
async fn native_node_child() {
    let id: u64 = std::env::var("PD_NATIVE_ID").unwrap().parse().unwrap();
    let cluster: u64 = std::env::var("PD_NATIVE_CLUSTER").unwrap().parse().unwrap();
    let directory = std::env::var("PD_NATIVE_DIR").unwrap();
    let client_listener =
        TcpListener::bind(listen_address(&std::env::var("PD_NATIVE_CLIENT").unwrap()))
            .await
            .unwrap();
    let voters: BTreeMap<_, _> = std::env::var("PD_NATIVE_PEERS")
        .unwrap()
        .split(',')
        .enumerate()
        .map(|(index, address)| {
            (
                index as u64 + 1,
                BasicNode::new(listen_address(address).to_string()),
            )
        })
        .collect();
    let peer = TcpListener::bind(listen_address(&voters[&id].addr))
        .await
        .unwrap();
    let isolated = Arc::new(AtomicBool::new(false));
    let store = NativeMetaStore::open(
        Path::new(&directory),
        cluster,
        id,
        config(),
        Loopback::with_gate(isolated.clone()),
    )
    .await
    .unwrap();
    let node = Arc::new(Node { store, isolated });
    let peers = Router::new()
        .route(
            "/append",
            post(
                |State(node): State<Arc<Node>>,
                 Json(req): Json<AppendEntriesRequest<NativeRaft>>| async move {
                    node.accept_peer()?;
                    node.store
                        .consensus()
                        .append_entries(req)
                        .await
                        .map(Json)
                        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
                },
            ),
        )
        .route(
            "/vote",
            post(
                |State(node): State<Arc<Node>>, Json(req): Json<VoteRequest<u64>>| async move {
                    node.accept_peer()?;
                    node.store
                        .consensus()
                        .vote(req)
                        .await
                        .map(Json)
                        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
                },
            ),
        )
        .route(
            "/snapshot",
            post(
                |State(node): State<Arc<Node>>,
                 Json(req): Json<InstallSnapshotRequest<NativeRaft>>| async move {
                    node.accept_peer()?;
                    node.store
                        .consensus()
                        .install_snapshot(req)
                        .await
                        .map(Json)
                        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
                },
            ),
        )
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .with_state(node.clone());
    let client = Router::new()
        .route("/status", post(status))
        .route("/v3/maintenance/status", post(bench_status))
        .route("/v3/kv/txn", post(bench_write))
        .route("/v3/kv/range", post(bench_read))
        .route("/write", post(write))
        .route("/read", post(read))
        .route("/compact", post(compact))
        .route(
            "/isolate",
            post(
                |State(node): State<Arc<Node>>, Json(isolated): Json<bool>| async move {
                    node.isolated.store(isolated, Ordering::Release);
                    Json(json!({"isolated": isolated}))
                },
            ),
        )
        .with_state(node.clone());

    let mut termination = signal(SignalKind::terminate()).unwrap();
    let peer_server = tokio::spawn(async move { axum::serve(peer, peers).await.unwrap() });
    let client_server =
        tokio::spawn(async move { axum::serve(client_listener, client).await.unwrap() });
    if id == 1 && std::env::var_os("PD_NATIVE_INITIALIZE").is_some() {
        node.store.consensus().initialize(voters).await.unwrap();
    }
    termination.recv().await;
    client_server.abort();
    peer_server.abort();
    let _ = client_server.await;
    let _ = peer_server.await;
    node.store.consensus().shutdown().await.unwrap();
}
