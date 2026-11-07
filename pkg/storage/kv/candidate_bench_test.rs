// Copyright 2026 AsterSQL.
//! Loopback-only subprocess for the shared SQLite/etcd workload runner.

use std::collections::BTreeMap;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use openraft::error::{InstallSnapshotError, RPCError, RaftError, Unreachable};
use openraft::network::RPCOption;
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::{BasicNode, Config, Raft, RaftNetwork, RaftNetworkFactory, SnapshotPolicy};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

use super::candidate::Candidate;
use super::candidate_profile_test::{event, install};
use super::candidate_raft::{PutPair, RaftDisk, Replay, Type};

type PeerClient = Client<HttpConnector, Full<Bytes>>;
pub(super) type RpcResult<T, E = openraft::error::Infallible> =
    Result<T, RPCError<u64, BasicNode, RaftError<u64, E>>>;

pub(super) struct Peer {
    client: PeerClient,
    address: String,
    isolated: Arc<AtomicBool>,
}

impl Peer {
    async fn send<Req: Serialize, Resp: DeserializeOwned, E: std::error::Error>(
        &self,
        path: &str,
        payload: Req,
    ) -> RpcResult<Resp, E> {
        let result = async {
            if self.isolated.load(Ordering::Acquire) {
                return Err(io::Error::other("isolated test peer").into());
            }
            let bytes = serde_json::to_vec(&payload)?;
            let request = Request::post(format!("http://{}/{path}", self.address))
                .header("content-type", "application/json")
                .body(Full::new(Bytes::from(bytes)))?;
            let response = self.client.request(request).await?;
            if !response.status().is_success() {
                return Err(io::Error::other(format!("peer HTTP {}", response.status())).into());
            }
            let body = response.into_body().collect().await?.to_bytes();
            Ok(serde_json::from_slice(&body)?)
        }
        .await;
        result.map_err(|error: Box<dyn std::error::Error + Send + Sync>| {
            Unreachable::new(&io::Error::other(error)).into()
        })
    }
}

impl<C> RaftNetworkFactory<C> for Loopback
where
    C: openraft::RaftTypeConfig<NodeId = u64, Node = BasicNode, Entry = openraft::Entry<C>>,
{
    type Network = Peer;

    async fn new_client(&mut self, _: u64, node: &BasicNode) -> Peer {
        Peer {
            client: self.0.clone(),
            address: node.addr.clone(),
            isolated: self.1.clone(),
        }
    }
}

pub(super) struct Loopback(PeerClient, Arc<AtomicBool>);

impl Loopback {
    pub(super) fn new() -> Self {
        Self::with_gate(Arc::new(AtomicBool::new(false)))
    }
    pub(super) fn with_gate(isolated: Arc<AtomicBool>) -> Self {
        Self(Client::builder(TokioExecutor::new()).build_http(), isolated)
    }
}

impl<C> RaftNetwork<C> for Peer
where
    C: openraft::RaftTypeConfig<NodeId = u64, Node = BasicNode, Entry = openraft::Entry<C>>,
{
    async fn append_entries(
        &mut self,
        req: AppendEntriesRequest<C>,
        _: RPCOption,
    ) -> RpcResult<AppendEntriesResponse<u64>> {
        let index = req.entries.last().map(|entry| entry.log_id.index);
        let started = Instant::now();
        let result = self.send("append", req).await;
        if let Some(index) = index {
            event("replicate", index, started);
        }
        result
    }

    async fn vote(&mut self, req: VoteRequest<u64>, _: RPCOption) -> RpcResult<VoteResponse<u64>> {
        self.send("vote", req).await
    }

    async fn install_snapshot(
        &mut self,
        req: InstallSnapshotRequest<C>,
        _: RPCOption,
    ) -> RpcResult<InstallSnapshotResponse<u64>, InstallSnapshotError> {
        self.send("snapshot", req).await
    }
}

#[derive(Clone)]
struct ClientState {
    raft: Raft<Type>,
    model: ReadModel,
    id: u64,
}

#[derive(Clone)]
enum ReadModel {
    Materialized(RaftDisk),
    Replayed(Replay),
}

#[derive(Deserialize)]
struct Put {
    key: String,
    value: String,
}

#[derive(Deserialize)]
struct Op {
    request_put: Put,
}

#[derive(Deserialize)]
struct Txn {
    success: [Op; 2],
}

#[derive(Deserialize)]
struct Point {
    key: String,
}

async fn write(
    State(state): State<ClientState>,
    Json(txn): Json<Txn>,
) -> Result<Json<Value>, StatusCode> {
    let started = Instant::now();
    let [left, right] = txn.success;
    let decode = |op: Op| -> Result<_, StatusCode> {
        let key = STANDARD
            .decode(op.request_put.key)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        let value = STANDARD
            .decode(op.request_put.value)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        if !key.starts_with(b"/acceptance/") || value.len() != 256 {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok((key, value))
    };
    let response = state
        .raft
        .client_write(PutPair([decode(left)?, decode(right)?]))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    event("http-write", response.log_id.index, started);
    Ok(Json(
        json!({"succeeded": true, "header": {"revision": response.data}}),
    ))
}

async fn read(
    State(state): State<ClientState>,
    Json(point): Json<Point>,
) -> Result<Json<Value>, StatusCode> {
    let key = STANDARD
        .decode(point.key)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    state
        .raft
        .ensure_linearizable()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let value = match &state.model {
        ReadModel::Materialized(disk) => disk.value(&key),
        ReadModel::Replayed(model) => model.value(&key),
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let kvs = value
        .map(|value| json!({"value": STANDARD.encode(value)}))
        .into_iter()
        .collect::<Vec<_>>();
    Ok(Json(json!({"kvs": kvs})))
}

async fn status(State(state): State<ClientState>) -> Json<Value> {
    let leader = state.raft.current_leader().await.map(|id| id.to_string());
    Json(json!({"leader": leader, "header": {"member_id": state.id.to_string()}}))
}

pub(super) fn listen_address(text: &str) -> SocketAddr {
    let addr: SocketAddr = text.parse().unwrap();
    assert!(addr.ip().is_loopback());
    addr
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "subprocess entry for the shared etcd workload runner"]
async fn candidate_bench_child() {
    let id: u64 = std::env::var("PD_CANDIDATE_ID").unwrap().parse().unwrap();
    let path = std::env::var("PD_CANDIDATE_DB").unwrap();
    let client = TcpListener::bind(listen_address(
        &std::env::var("PD_CANDIDATE_CLIENT").unwrap(),
    ))
    .await
    .unwrap();
    let peers = std::env::var("PD_CANDIDATE_PEERS").unwrap();
    let voters: BTreeMap<_, _> = peers
        .split(',')
        .enumerate()
        .map(|(i, addr)| {
            (
                i as u64 + 1,
                BasicNode::new(listen_address(addr).to_string()),
            )
        })
        .collect();
    assert_eq!(voters.len(), 3);
    let peer = TcpListener::bind(listen_address(&voters[&id].addr))
        .await
        .unwrap();
    install();
    let candidate = Candidate::open(Path::new(&path)).unwrap();
    if let Ok(pages) = std::env::var("PD_CANDIDATE_WAL_PAGES") {
        let pages: u32 = pages.parse().unwrap();
        candidate
            .db
            .pragma_update(None, "wal_autocheckpoint", pages)
            .unwrap();
    }
    let disk = RaftDisk::new(candidate);
    let config = Config {
        heartbeat_interval: 30,
        election_timeout_min: 120,
        election_timeout_max: 240,
        snapshot_policy: SnapshotPolicy::Never,
        ..Default::default()
    }
    .validate()
    .unwrap();
    let network = Loopback::new();
    let config = Arc::new(config);
    let (raft, model) = if std::env::var_os("PD_CANDIDATE_REPLAY").is_some() {
        let (disk, model) = Replay::open(disk).unwrap();
        let raft = Raft::new(id, config, network, disk, model.clone())
            .await
            .unwrap();
        (raft, ReadModel::Replayed(model))
    } else {
        let raft = Raft::new(id, config, network, disk.clone(), disk.clone())
            .await
            .unwrap();
        (raft, ReadModel::Materialized(disk))
    };
    let peer_router =
        Router::new()
            .route(
                "/append",
                post(
                    |State(raft): State<Raft<Type>>,
                     Json(req): Json<AppendEntriesRequest<Type>>| async move {
                        raft.append_entries(req)
                            .await
                            .map(Json)
                            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
                    },
                ),
            )
            .route(
                "/vote",
                post(
                    |State(raft): State<Raft<Type>>, Json(req): Json<VoteRequest<u64>>| async move {
                        raft.vote(req)
                            .await
                            .map(Json)
                            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
                    },
                ),
            )
            .with_state(raft.clone());
    let client_router = Router::new()
        .route("/v3/maintenance/status", post(status))
        .route("/v3/kv/txn", post(write))
        .route("/v3/kv/range", post(read))
        .with_state(ClientState {
            raft: raft.clone(),
            model,
            id,
        });
    let mut termination = signal(SignalKind::terminate()).unwrap();
    let peer_server = tokio::spawn(async move { axum::serve(peer, peer_router).await.unwrap() });
    let client_server =
        tokio::spawn(async move { axum::serve(client, client_router).await.unwrap() });
    if id == 1 && std::env::var_os("PD_CANDIDATE_INITIALIZE").is_some() {
        raft.initialize(voters).await.unwrap();
    }
    termination.recv().await;
    client_server.abort();
    peer_server.abort();
    raft.shutdown().await.unwrap();
}
