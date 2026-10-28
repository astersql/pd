// Copyright 2026 AsterSQL.
use super::node_test::fixture;
use super::*;
use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_server::ClusterReadService;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

async fn rust_scenario(mode: &str) -> pdpb::RegionHeartbeatResponse {
    let mut fixture = fixture::Fixture::start();
    let failure = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let store = Arc::new(ConfigFailure {
        inner: Arc::new(fixture.store().await),
        fail: failure.clone(),
    });
    let endpoint = StorageEndpoint::new(store.clone());
    crate::node_test::seed_bootstrap(store.clone(), 517).await;
    let paths = ClusterPaths::new(517);
    endpoint
        .save_config(
            &paths,
            &serde_json::json!({
                "replication":{"max-replicas":2,"enable-placement-rules":"false"},
                "schedule":{"schedulers-v2":[{"type":"evict-leader","args":["1"]},{"type":"balance-leader","disable":true},{"type":"balance-region","disable":true},{"type":"hot-region","disable":true},{"type":"evict-slow-store","disable":true},{"type":"evict-stopping-store","disable":true}]}
            }),
        )
        .await
        .unwrap();
    if mode == "defaults" {
        endpoint.save_config(&paths,&serde_json::json!({"replication":{"max-replicas":2,"enable-placement-rules":"false"},"schedule":{"schedulers-v2":[{"type":"evict-leader","args":["1"]}]}})).await.unwrap();
    }
    let peer_values = (1..=2)
        .map(|id| astersql_pd_core::Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    endpoint
        .save_region(
            &paths,
            &astersql_pd_core::RegionMeta {
                id: 7,
                key_range: astersql_pd_core::KeyRange {
                    start_key: b"a".to_vec(),
                    end_key: b"b".to_vec(),
                },
                peers: peer_values,
                epoch: Some(astersql_pd_core::RegionEpoch {
                    version: 2,
                    conf_ver: 2,
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut config_server = None;
    let status_address = if mode == "store-update" {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        config_server = Some(tokio::spawn(async move {
            axum::serve(listener,axum::Router::new().route("/config",axum::routing::get(||async {
                axum::Json(serde_json::json!({"coprocessor":{"region-max-size":"256MiB"},"storage":{"engine":"raft-kv2"}}))
            }))).await.unwrap();
        }));
        address
    } else {
        String::new()
    };
    let cluster = BasicCluster::new();
    for id in 1..=2 {
        let mut info = StoreInfo::new(StoreMeta {
            id,
            status_address: if id == 1 {
                status_address.clone()
            } else {
                String::new()
            },
            node_state: NodeState::Serving,
            ..Default::default()
        });
        info.set_last_heartbeat_unix_nanos(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as i64,
        );
        cluster.stores.put_store(Arc::new(info));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let parent = CancellationToken::new();
    let registry = Arc::new(prometheus::Registry::new());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
        .with_cluster(cluster)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 517,
                member_id: 17,
                name: "schedule".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();
    assert!(
        !host.schedule_initialized(),
        "restored regions must wait for heartbeat preparation"
    );
    assert!(
        host.store(1)
            .await
            .unwrap()
            .unwrap()
            .allow_leader_transfer_in()
    );
    if mode == "store-update" {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !host.store_config().await.unwrap().is_raft_kv2() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            host.store_config().await.unwrap().get_region_max_size(),
            256
        );
    }
    if mode == "cancel" {
        parent.cancel();
        node.wait().await.unwrap();
        assert!(!host.schedule_initialized());
        assert!(
            endpoint
                .load_scheduler_config(&paths, "evict-leader-scheduler")
                .await
                .unwrap()
                .is_empty()
        );
        assert!(registry.gather().is_empty());
        drop(endpoint);
        assert_eq!(Arc::strong_count(&store), 1);
        return Default::default();
    }
    if mode == "storage-fault" {
        fixture.command("stop", "stopped");
        // The prepare gate still owns no policies; a new command cannot be accepted.
        parent.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(10), node.wait())
            .await
            .unwrap();
        assert!(!host.schedule_initialized());
        assert!(registry.gather().is_empty());
        fixture.command("start", "restarted");
        assert!(
            endpoint
                .load_scheduler_config(&paths, "evict-leader-scheduler")
                .await
                .unwrap()
                .is_empty()
        );
        drop(endpoint);
        assert_eq!(Arc::strong_count(&store), 1);
        return Default::default();
    }
    let mut client = PdClient::connect(url).await.unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let mut output = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let peers: Vec<_> = (1..=2)
        .map(|id| metapb::Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let report = pdpb::RegionHeartbeatRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 517,
            ..Default::default()
        }),
        region: Some(metapb::Region {
            id: 7,
            start_key: b"a".to_vec(),
            end_key: b"b".to_vec(),
            peers: peers.clone(),
            region_epoch: Some(metapb::RegionEpoch {
                version: 2,
                conf_ver: 2,
            }),
            ..Default::default()
        }),
        leader: Some(peers[0].clone()),
        approximate_size: 1048576,
        ..Default::default()
    };
    if mode == "write-fault" {
        failure.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    tx.send(report.clone()).await.unwrap();
    let command = tokio::time::timeout(Duration::from_secs(8), output.message()).await;
    assert!(host.schedule_initialized());
    if mode == "write-fault" {
        assert!(
            host.schedule_error().is_some(),
            "final config persistence failure must remain observable"
        );
    } else {
        assert!(
            host.schedule_error().is_none(),
            "{:?}",
            host.schedule_error()
        );
    }
    if mode == "store-update" {
        let mut config = endpoint
            .load_config::<astersql_pd_server_config::PersistedConfig>(&paths)
            .await
            .unwrap()
            .unwrap();
        config.store.adjust();
        assert_eq!(
            config.store.get_region_max_size(),
            256,
            "scheduler restore must preserve the latest committed TiKV configuration"
        );
        assert_eq!(config.config.schedule.store_limit_version, "v2");
        assert_eq!(
            host.store_config().await.unwrap().get_region_max_size(),
            256
        );
    }
    let command = command
        .expect("restored scheduler must deliver an operator over the real heartbeat stream")
        .unwrap()
        .unwrap();
    assert_eq!(command.region_id, 7);
    assert_eq!(command.header.as_ref().unwrap().cluster_id, 517);
    assert_eq!(
        command
            .transfer_leader
            .as_ref()
            .unwrap()
            .peer
            .as_ref()
            .unwrap()
            .store_id,
        2
    );
    // A replacement stream must receive subsequent pushes for the same operator.
    let (new_tx, new_rx) = tokio::sync::mpsc::channel(8);
    let mut replacement = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(new_rx))
        .await
        .unwrap()
        .into_inner();
    new_tx.send(report.clone()).await.unwrap();
    let rebound = tokio::time::timeout(Duration::from_secs(5), replacement.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(rebound.region_id, 7);
    assert_eq!(rebound.transfer_leader.unwrap().peer.unwrap().store_id, 2);
    let mut stale = report.clone();
    stale
        .region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .version = 0;
    new_tx.send(stale).await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let response = replacement.message().await.unwrap().unwrap();
            if response.header.as_ref().is_some_and(|h| h.error.is_some()) {
                break response;
            }
        }
    })
    .await
    .unwrap();
    assert!(
        error
            .header
            .unwrap()
            .error
            .unwrap()
            .message
            .starts_with("region is stale")
    );
    assert_eq!(
        host.snapshot().await.unwrap().regions[0]
            .meta()
            .epoch
            .unwrap()
            .version,
        2
    );
    parent.cancel();
    tokio::time::timeout(Duration::from_secs(5), node.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(host.snapshot().await.is_err());
    assert!(registry.gather().is_empty());
    assert!(
        endpoint
            .load_scheduler_config(&paths, "evict-leader-scheduler")
            .await
            .unwrap()
            .len()
            > 0
    );
    assert_eq!(
        endpoint
            .load_region(&paths, 7)
            .await
            .unwrap()
            .unwrap()
            .peers
            .len(),
        2
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(5), replacement.message())
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    if let Some(server) = config_server {
        server.abort();
        let _ = server.await;
    }
    drop((tx, output, new_tx, replacement, client, endpoint));
    assert_eq!(Arc::strong_count(&store), 1);
    normalized_command(command)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_017_behavior() {
    rust_scenario("success").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_017_default_schedulers() {
    rust_scenario("defaults").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_017_cancel_before_preparation() {
    rust_scenario("cancel").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_017_storage_fault_and_cancel() {
    rust_scenario("storage-fault").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the read-only supervisor and existing Go PD binary"]
async fn readonly_schedule_coordinator_differential() {
    use astersql_pd_compat_tests::harness::Server;
    let rust = rust_scenario("defaults").await;
    let binary = std::path::PathBuf::from(std::env::var("PD_COMPAT_GO_BIN").unwrap());
    let mut server = Server::start(&binary).unwrap();
    server.ready().unwrap();
    let mut client = PdClient::new(
        tonic::transport::Endpoint::from_shared(server.url.clone())
            .unwrap()
            .timeout(Duration::from_secs(10))
            .connect()
            .await
            .unwrap(),
    );
    let cluster_id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id,
        ..Default::default()
    });
    let peers = (1..=2)
        .map(|id| metapb::Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let region = metapb::Region {
        id: 7,
        peers: vec![peers[0].clone()],
        region_epoch: Some(metapb::RegionEpoch {
            version: 1,
            conf_ver: 1,
        }),
        ..Default::default()
    };
    let stores = (1..=2)
        .map(|id| metapb::Store {
            id,
            address: format!("127.0.0.1:{}", 29990 + id),
            version: "8.5.0".into(),
            node_state: metapb::NodeState::Serving as i32,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let response = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(stores[0].clone()),
            region: Some(region.clone()),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    assert!(
        client
            .put_store(pdpb::PutStoreRequest {
                header: header.clone(),
                store: Some(stores[1].clone())
            })
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_none()
    );
    curl_json(
        &server.url,
        "POST",
        "/pd/api/v1/config",
        Some(r#"{"replication.max-replicas":2,"replication.enable-placement-rules":"false"}"#),
    );
    for id in 1..=2 {
        assert!(
            client
                .store_heartbeat(pdpb::StoreHeartbeatRequest {
                    header: header.clone(),
                    stats: Some(pdpb::StoreStats {
                        store_id: id,
                        capacity: 1 << 40,
                        available: 1 << 39,
                        region_count: 1,
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .error
                .is_none()
        );
    }
    let mut region = region;
    region.peers = peers.clone();
    region.start_key = b"a".to_vec();
    region.end_key = b"b".to_vec();
    region.region_epoch.as_mut().unwrap().version = 2;
    region.region_epoch.as_mut().unwrap().conf_ver = 2;
    let report = pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(region),
        leader: Some(peers[0].clone()),
        approximate_size: 1048576,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    tx.send(report.clone()).await.unwrap();
    let mut stream_client = client.clone();
    let pre_output = tokio::spawn(async move {
        stream_client
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let _prepared_schedulers = loop {
        let names = curl_json(&server.url, "GET", "/pd/api/v1/schedulers", None);
        if names.as_array().is_some_and(|a| !a.is_empty()) {
            break names;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Go coordinator did not prepare"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    curl_json(
        &server.url,
        "POST",
        "/pd/api/v1/schedulers",
        Some(r#"{"name":"evict-leader-scheduler","store_id":1}"#),
    );
    let pre_output = tokio::time::timeout(Duration::from_secs(10), pre_output)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop((tx, pre_output, client));
    server.restart_with_args(&binary, &[]).unwrap();
    server.ready().unwrap();
    let mut client = PdClient::new(
        tonic::transport::Endpoint::from_shared(server.url.clone())
            .unwrap()
            .timeout(Duration::from_secs(10))
            .connect()
            .await
            .unwrap(),
    );
    for id in 1..=2 {
        assert!(
            client
                .store_heartbeat(pdpb::StoreHeartbeatRequest {
                    header: header.clone(),
                    stats: Some(pdpb::StoreStats {
                        store_id: id,
                        capacity: 1 << 40,
                        available: 1 << 39,
                        region_count: 1,
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .error
                .is_none()
        );
    }
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    tx.send(report.clone()).await.unwrap();
    let mut output = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let command = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let command = output.message().await.unwrap().unwrap();
            eprintln!("Go heartbeat output: {command:?}");
            if command.transfer_leader.is_some() {
                break command;
            }
        }
    })
    .await
    .unwrap_or_else(|e| {
        panic!(
            "Go restored scheduler command timeout: {e}; {}",
            server.logs()
        )
    });
    assert_eq!(command.header.as_ref().unwrap().cluster_id, cluster_id);
    let go = normalized_command(command);
    eprintln!("restored command comparison: Go={go:?}, Rust={rust:?}");
    assert_eq!(
        go, rust,
        "restored Go and Rust policies must produce the same command"
    );
    let mut stale = report;
    stale
        .region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .version = 0;
    tx.send(stale).await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let response = output.message().await.unwrap().unwrap();
            if response.header.as_ref().is_some_and(|h| h.error.is_some()) {
                break response;
            }
        }
    })
    .await
    .unwrap();
    assert!(
        error
            .header
            .unwrap()
            .error
            .unwrap()
            .message
            .contains("region is stale")
    );
    let names = curl_json(&server.url, "GET", "/pd/api/v1/schedulers", None);
    let mut names = names
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            "balance-hot-region-scheduler",
            "balance-leader-scheduler",
            "balance-region-scheduler",
            "evict-leader-scheduler",
            "evict-slow-store-scheduler",
            "evict-stopping-store-scheduler"
        ]
    );
    drop((tx, output, client));
    server.stop().unwrap();
}
fn curl_json(url: &str, method: &str, path: &str, body: Option<&str>) -> serde_json::Value {
    let mut cmd = std::process::Command::new("curl");
    cmd.args([
        "-fsS",
        "--max-time",
        "10",
        "-X",
        method,
        "-H",
        "Content-Type: application/json",
    ]);
    if let Some(body) = body {
        cmd.args(["-d", body]);
    }
    let output = cmd.arg(format!("{url}{path}")).output().unwrap();
    assert!(output.status.success(), "{method} {path}: {:?}", output);
    serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null)
}

#[tokio::test(start_paused = true)]
async fn task_017_deterministic_tick_and_shared_state() {
    use astersql_pd_core::{Peer, RegionEpoch, RegionInfo, RegionMeta};
    use astersql_pd_server::region_heartbeat_adapter::RegionHeartbeatService;
    fn heartbeat_region(
        meta: RegionMeta,
        leader: Option<Peer>,
        down: Vec<astersql_pd_core::DownPeer>,
        pending: Vec<Peer>,
    ) -> RegionInfo {
        let mut region = RegionInfo::new(meta, leader, down, pending);
        region.set_source(astersql_pd_core::RegionSource::Heartbeat);
        region
    }
    let store = Arc::new(astersql_pd_storage::MemoryStore::default());
    let endpoint = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(518);
    endpoint.save_config(&paths,&serde_json::json!({"replication":{"max-replicas":2,"enable-placement-rules":"false"},
        "schedule":{"schedulers-v2":[{"type":"evict-leader","args":["1"]},{"type":"balance-leader","disable":true},{"type":"balance-region","disable":true},{"type":"hot-region","disable":true},{"type":"evict-slow-store","disable":true},{"type":"evict-stopping-store","disable":true}]}})).await.unwrap();
    let peers = (1..=2)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let meta = RegionMeta {
        id: 8,
        peers: peers.clone(),
        epoch: Some(RegionEpoch {
            version: 1,
            conf_ver: 1,
        }),
        ..Default::default()
    };
    endpoint.save_region(&paths, &meta).await.unwrap();
    let cluster = BasicCluster::new();
    let observed = cluster.clone();
    for id in 1..=2 {
        let mut info = StoreInfo::new(StoreMeta {
            id,
            node_state: NodeState::Serving,
            ..Default::default()
        });
        info.set_last_heartbeat_unix_nanos(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as i64,
        );
        cluster.stores.put_store(Arc::new(info));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let parent = CancellationToken::new();
    let registry = Arc::new(prometheus::Registry::new());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
        .with_cluster(cluster)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 518,
                member_id: 18,
                name: "clock".into(),
                client_url: url,
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();
    let mut status = host.watch_schedule();
    while status.borrow().phase == crate::cluster_host::SchedulePhase::Starting {
        status.changed().await.unwrap();
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    host.bind_heartbeat(1, Arc::new(tx)).await.unwrap();
    assert!(!host.schedule_initialized());
    assert!(rx.try_recv().is_err());
    host.region_heartbeat(heartbeat_region(
        meta.clone(),
        Some(peers[0].clone()),
        vec![],
        vec![],
    ))
    .await
    .unwrap();
    assert_eq!(
        observed
            .regions
            .get_region(8)
            .unwrap()
            .leader()
            .unwrap()
            .store_id,
        1
    );
    // Advance only after the actor publishes each lifecycle boundary. A fixed
    // number of yield_now calls cannot synchronize a blocking-pool actor.
    tokio::time::advance(Duration::from_secs(3)).await;
    while status.borrow().phase != crate::cluster_host::SchedulePhase::Running {
        status.changed().await.unwrap();
    }
    tokio::time::advance(Duration::from_millis(10)).await;
    while status.borrow().ticks == 0 {
        status.changed().await.unwrap();
    }
    let command = rx
        .recv()
        .await
        .expect("completed scheduling tick must deliver its real operator");
    assert_eq!(command.region_id, 8);
    assert_eq!(command.target_peer.store_id, 1);
    assert!(host.schedule_initialized());
    host.region_heartbeat(heartbeat_region(
        meta,
        Some(peers[1].clone()),
        vec![],
        vec![],
    ))
    .await
    .unwrap();
    assert_eq!(
        observed
            .regions
            .get_region(8)
            .unwrap()
            .leader()
            .unwrap()
            .store_id,
        2
    );
    parent.cancel();
    node.wait().await.unwrap();
    assert_eq!(
        status.borrow().phase,
        crate::cluster_host::SchedulePhase::Stopped
    );
    assert!(
        observed
            .stores
            .get_store(1)
            .unwrap()
            .allow_leader_transfer_in(),
        "cleanup must follow worker join"
    );
    assert!(registry.gather().is_empty());
    drop(endpoint);
    assert_eq!(Arc::strong_count(&store), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_017_transaction_failure_is_nonfatal() {
    rust_scenario("write-fault").await;
}

use astersql_pd_storage::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
struct ConfigFailure {
    inner: Arc<dyn MetaStore>,
    fail: Arc<AtomicBool>,
}
#[async_trait]
impl MetaStore for ConfigFailure {
    async fn read(&self, r: Read) -> Result<Snapshot> {
        self.inner.read(r).await
    }
    async fn transact(&self, c: Vec<Compare>, w: Vec<Mutation>) -> Result<Commit> {
        if w.iter()
            .any(|m| matches!(m, Mutation::Put { key, .. } if key.ends_with(b"/config")))
            && self.fail.load(Ordering::SeqCst)
        {
            return Err(StoreError::Unavailable);
        }
        self.inner.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> Result<Commit> {
        self.inner.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        self.inner.transact_raw(c, t, e).await
    }
    async fn watch(&self, p: Vec<u8>, r: Revision) -> Result<Box<dyn MetaWatch>> {
        self.inner.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> Result<Lease> {
        self.inner.grant(t).await
    }
    async fn keep_alive(&self, l: LeaseId) -> Result<Lease> {
        self.inner.keep_alive(l).await
    }
    async fn revoke(&self, l: LeaseId) -> Result<()> {
        self.inner.revoke(l).await
    }
    async fn ttl(&self, l: LeaseId) -> Result<i64> {
        self.inner.ttl(l).await
    }
    async fn compact(&self, r: Revision) -> Result<()> {
        self.inner.compact(r).await
    }
}

fn normalized_command(mut command: pdpb::RegionHeartbeatResponse) -> pdpb::RegionHeartbeatResponse {
    // Compare every known wire field; cluster identity is independently checked before normalization.
    command.header.as_mut().unwrap().cluster_id = 0;
    command
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_017_restore_preserves_concurrent_store_configuration() {
    rust_scenario("store-update").await;
}
