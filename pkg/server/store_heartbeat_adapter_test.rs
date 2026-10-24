// Copyright 2026 AsterSQL.
use super::store_heartbeat_adapter::*;
use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta};
use std::sync::Arc;
#[test]
fn state_task_47_store_heartbeat_adapter_state() {
    let mut cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    let stats = astersql_pd_core::StoreStats {
        store_id: 1,
        capacity: 100,
        ..Default::default()
    };
    assert_eq!(
        apply_store_heartbeat(&mut cluster, None, &stats, 123),
        Ok(NodeState::Serving)
    );
    assert_eq!(cluster.stores.get_store(1).unwrap().store_stats(), stats);
}

use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_server::Pd},
};
use astersql_pd_statistics::HotStat;
use prost::Message;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tonic::Request;
struct Host {
    identity: crate::Identity,
    cluster: Mutex<BasicCluster>,
    hot: Mutex<Option<HotStat>>,
    enabled: bool,
    bootstrapped: bool,
    reply: Option<Result<NodeState, StoreHeartbeatError>>,
}
impl Host {
    fn new(hot: Option<HotStat>) -> Self {
        let cluster = BasicCluster::new();
        for (id, state, node_state) in [
            (1, astersql_pd_core::StoreState::Up, NodeState::Serving),
            (
                3,
                astersql_pd_core::StoreState::Tombstone,
                NodeState::Removed,
            ),
        ] {
            cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                state,
                node_state,
                ..Default::default()
            })));
        }
        Self {
            identity: crate::Identity {
                cluster_id: 42,
                member_id: 1,
                name: "state47".into(),
                client_url: String::new(),
                peer_url: String::new(),
            },
            cluster: Mutex::new(cluster),
            hot: Mutex::new(hot),
            enabled: true,
            bootstrapped: true,
            reply: None,
        }
    }
}
#[async_trait::async_trait]
impl StoreHeartbeatService for Host {
    async fn store_heartbeat(
        &self,
        stats: astersql_pd_core::StoreStats,
    ) -> Result<NodeState, StoreHeartbeatError> {
        if let Some(reply) = &self.reply {
            return reply.clone();
        }
        if !self.bootstrapped {
            return Err(StoreHeartbeatError::NotBootstrapped);
        }
        apply_store_heartbeat(
            &mut self.cluster.lock().unwrap(),
            self.hot.lock().unwrap().as_ref(),
            &stats,
            1_000_000_000_000,
        )
    }
}
#[async_trait::async_trait]
impl crate::PdServices for Host {
    fn identity(&self) -> &crate::Identity {
        &self.identity
    }
    fn store_heartbeat_service(&self) -> Option<&dyn StoreHeartbeatService> {
        self.enabled.then_some(self)
    }
    async fn healthy(&self) -> Result<(), String> {
        Ok(())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("unused".into())
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("unused".into())
    }
}
fn request(stats: Option<pdpb::StoreStats>) -> pdpb::StoreHeartbeatRequest {
    // Exercise actual protobuf decoding, including optional-message presence.
    let bytes = pdpb::StoreHeartbeatRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 42,
            ..Default::default()
        }),
        stats,
        ..Default::default()
    }
    .encode_to_vec();
    pdpb::StoreHeartbeatRequest::decode(bytes.as_slice()).unwrap()
}
fn grpc(host: Arc<Host>) -> crate::grpc::Grpc {
    crate::grpc::Grpc {
        services: host,
        cancel: CancellationToken::new(),
    }
}
#[tokio::test]
async fn state_task_47_store_heartbeat_adapter_wire_errors() {
    let host = Arc::new(Host::new(None));
    let rpc = grpc(host.clone());
    let missing = rpc
        .store_heartbeat(Request::new(request(None)))
        .await
        .unwrap_err();
    assert_eq!(missing.code(), tonic::Code::Unknown);
    assert!(
        missing
            .message()
            .starts_with("invalid store heartbeat command")
    );
    for (id, kind, message) in [
        (99, pdpb::ErrorType::Unknown, "store 99 not found"),
        (3, pdpb::ErrorType::StoreTombstone, "store is tombstone"),
    ] {
        let response = rpc
            .store_heartbeat(Request::new(request(Some(pdpb::StoreStats {
                store_id: id,
                ..Default::default()
            }))))
            .await
            .unwrap()
            .into_inner();
        let header = response.header.unwrap();
        assert_eq!(header.cluster_id, 42);
        let error = header.error.unwrap();
        assert_eq!(error.r#type, kind as i32);
        assert_eq!(error.message, message);
        assert_eq!(response.state, 0);
    }
    assert_eq!(
        host.cluster
            .lock()
            .unwrap()
            .stores
            .get_store(3)
            .unwrap()
            .last_heartbeat_unix_nanos(),
        0
    );
    let mut bad = request(Some(pdpb::StoreStats::default()));
    bad.header = None;
    assert_eq!(
        rpc.store_heartbeat(Request::new(bad))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
}
#[tokio::test]
async fn state_task_47_store_heartbeat_adapter_defaults() {
    let host = Arc::new(Host::new(None));
    let rpc = grpc(host.clone());
    let stats = pdpb::StoreStats {
        store_id: 1,
        ..Default::default()
    };
    let result = rpc
        .store_heartbeat(Request::new(request(Some(stats.clone()))))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(result.state, metapb::NodeState::Serving as i32);
    assert!(result.header.unwrap().error.is_none());
    let actual = host
        .cluster
        .lock()
        .unwrap()
        .stores
        .get_store(1)
        .unwrap()
        .store_stats();
    assert_eq!(
        crate::core_store_adapter::store_stats_to_proto(actual),
        stats
    );
    assert!(host.hot.lock().unwrap().is_none());
}
#[tokio::test]
async fn state_task_47_store_heartbeat_adapter_opt_in_and_bootstrap() {
    let mut host = Host::new(None);
    host.enabled = false;
    let rpc = grpc(Arc::new(host));
    assert_eq!(
        rpc.store_heartbeat(Request::new(request(Some(pdpb::StoreStats::default()))))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unimplemented
    );
    let mut host = Host::new(None);
    host.bootstrapped = false;
    let rpc = grpc(Arc::new(host));
    let result = rpc
        .store_heartbeat(Request::new(request(Some(pdpb::StoreStats::default()))))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        result.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
}
#[tokio::test]
async fn state_task_47_store_heartbeat_adapter_multi_peer_network() {
    use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta};
    let hot = HotStat::new(
        &tokio::runtime::Handle::current(),
        &prometheus::Registry::new(),
        128,
    )
    .unwrap();
    let host = Arc::new(Host::new(Some(hot)));
    for id in [1, 2] {
        let peer = Peer {
            id: id + 10,
            store_id: 1,
            ..Default::default()
        };
        host.cluster
            .lock()
            .unwrap()
            .regions
            .put_region(Arc::new(RegionInfo::new(
                RegionMeta {
                    id,
                    peers: vec![peer.clone()],
                    key_range: KeyRange {
                        start_key: vec![id as u8],
                        end_key: vec![id as u8 + 1],
                    },
                    ..Default::default()
                },
                Some(peer),
                vec![],
                vec![],
            )));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let router = crate::router(host.clone(), cancel.clone());
    let stop = cancel.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap()
    });
    let mut client = pdpb::pd_client::PdClient::connect(url).await.unwrap();
    let stats = pdpb::StoreStats {
        store_id: 1,
        capacity: 100,
        available: 50,
        cpu_usages: vec![pdpb::RecordPair {
            key: "cpu".into(),
            value: 7,
        }],
        interval: Some(pdpb::TimeInterval {
            start_timestamp: 0,
            end_timestamp: 10,
        }),
        query_stats: Some(pdpb::QueryStats {
            get: 11,
            scan: 12,
            coprocessor: 13,
            gc: 100,
            ..Default::default()
        }),
        peer_stats: [1, 2]
            .into_iter()
            .map(|id| pdpb::PeerStat {
                region_id: id,
                read_bytes: 9999998,
                read_keys: 9999999,
                query_stats: Some(pdpb::QueryStats {
                    get: 9999997,
                    scan: 2,
                    coprocessor: 3,
                    gc: 100,
                    ..Default::default()
                }),
                cpu_stats: Some(pdpb::CpuStats {
                    unified_read: 7,
                    scheduler: 999,
                }),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let mut responses = Vec::new();
    for _ in 0..3 {
        responses.push(client.store_heartbeat(request(Some(stats.clone()))).await);
    }
    // Always cancel and join the listener before assertions, including RPC errors.
    drop(client);
    cancel.cancel();
    server.await.unwrap();
    let mut hot = host.hot.lock().unwrap().take().unwrap();
    let mut peers = Vec::new();
    for id in [1, 2] {
        peers.push(
            hot.hot_cache
                .get_hot_peer_stat(astersql_pd_statistics_utils::RWType::READ, id, 1)
                .await
                .unwrap(),
        );
    }
    hot.shutdown().await.unwrap();
    for response in responses {
        assert!(
            response
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .error
                .is_none()
        );
    }
    assert_eq!(
        crate::core_store_adapter::store_stats_to_proto(
            host.cluster
                .lock()
                .unwrap()
                .stores
                .get_store(1)
                .unwrap()
                .store_stats()
        ),
        stats
    );
    for peer in peers {
        let peer = peer.unwrap();
        assert_eq!(
            [
                peer.get_load(0),
                peer.get_load(1),
                peer.get_load(2),
                peer.get_load(3)
            ],
            [1000000., 1000000., 1000000., 7.]
        );
    }
}

#[tokio::test]
async fn state_task_47_store_heartbeat_adapter_response_state_and_errors() {
    for state in [
        NodeState::Preparing,
        NodeState::Serving,
        NodeState::Removing,
        NodeState::Removed,
        NodeState::Unknown(91),
    ] {
        let mut host = Host::new(None);
        host.reply = Some(Ok(state));
        let result = grpc(Arc::new(host))
            .store_heartbeat(Request::new(request(Some(pdpb::StoreStats::default()))))
            .await
            .unwrap()
            .into_inner();
        let expected = match state {
            NodeState::Preparing => 0,
            NodeState::Serving => 1,
            NodeState::Removing => 2,
            NodeState::Removed => 3,
            NodeState::Unknown(v) => v,
        };
        assert_eq!(result.state, expected);
        assert!(result.header.unwrap().error.is_none());
    }
    let mut host = Host::new(None);
    host.reply = Some(Err(StoreHeartbeatError::Other(
        "state backend unavailable".into(),
    )));
    let result = grpc(Arc::new(host))
        .store_heartbeat(Request::new(request(Some(pdpb::StoreStats::default()))))
        .await
        .unwrap()
        .into_inner();
    let error = result.header.unwrap().error.unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::Unknown as i32);
    assert_eq!(error.message, "state backend unavailable");
}
#[tokio::test]
async fn state_task_47_store_heartbeat_adapter_presence_and_controller_boundary() {
    let host = Arc::new(Host::new(None));
    let rpc = grpc(host.clone());
    let stats = pdpb::StoreStats {
        store_id: 1,
        interval: Some(pdpb::TimeInterval::default()),
        query_stats: Some(pdpb::QueryStats::default()),
        peer_stats: vec![pdpb::PeerStat {
            cpu_stats: Some(pdpb::CpuStats::default()),
            query_stats: Some(pdpb::QueryStats::default()),
            ..Default::default()
        }],
        ..Default::default()
    };
    rpc.store_heartbeat(Request::new(request(Some(stats.clone()))))
        .await
        .unwrap();
    assert_eq!(
        crate::core_store_adapter::store_stats_to_proto(
            host.cluster
                .lock()
                .unwrap()
                .stores
                .get_store(1)
                .unwrap()
                .store_stats()
        ),
        stats
    );
    for recovery in [true, false] {
        let mut req = request(Some(pdpb::StoreStats {
            store_id: 1,
            capacity: 999,
            ..Default::default()
        }));
        if recovery {
            req.store_report = Some(pdpb::StoreReport::default());
        } else {
            req.dr_autosync_status = Some(Default::default());
        }
        assert_eq!(
            rpc.store_heartbeat(Request::new(req))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::Unimplemented
        );
        assert_eq!(
            crate::core_store_adapter::store_stats_to_proto(
                host.cluster
                    .lock()
                    .unwrap()
                    .stores
                    .get_store(1)
                    .unwrap()
                    .store_stats()
            ),
            stats
        );
    }
    rpc.store_heartbeat(Request::new(request(Some(pdpb::StoreStats {
        store_id: 1,
        ..Default::default()
    }))))
    .await
    .unwrap();
    let raw = host
        .cluster
        .lock()
        .unwrap()
        .stores
        .get_store(1)
        .unwrap()
        .store_stats();
    assert!(raw.interval.is_none());
    assert!(raw.query_stats.is_none());
    assert!(raw.peer_stats.is_empty());
}
