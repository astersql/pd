// Copyright 2026 AsterSQL.
use crate::http_member_test::request;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

async fn observe(url: &str, store: &dyn MetaStore, id: u64) -> Value {
    let cluster = request(url, "GET", "/pd/api/v1/cluster", "");
    assert_eq!(cluster, (200, json!({"id":id,"max_peer_count":3})));
    let before = request(url, "GET", "/pd/api/v1/cluster/status", "");
    assert_eq!(
        before,
        (
            200,
            json!({"raft_bootstrap_time":"0001-01-01T00:00:00Z","is_initialized":false,"replication_status":""})
        )
    );
    let version = request(url, "GET", "/pd/api/v1/version", "");
    let status = request(url, "GET", "/pd/api/v1/status", "");
    assert_eq!(version.0, 200);
    assert_eq!(status.0, 200);
    assert_eq!(version.1.as_object().unwrap().len(), 4);
    assert_eq!(status.1.as_object().unwrap().len(), 5);
    for key in ["build_time", "hash", "branch"] {
        assert_eq!(version.1[key], "");
    }
    assert_eq!(version.1["version"], status.1["version"]);
    for key in ["version", "build_ts", "git_hash", "kernel_type"] {
        assert!(status.1[key].is_string());
    }
    assert_eq!(status.1["kernel_type"], "Classic");
    let start = status.1["start_timestamp"].as_i64().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(start > 0 && start <= now && now - start < 60);
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let mut region = metapb::Region {
        id: 2,
        peers: vec![metapb::Peer {
            id: 3,
            store_id: 1,
            ..Default::default()
        }],
        region_epoch: Some(metapb::RegionEpoch {
            conf_ver: 1,
            version: 1,
        }),
        ..Default::default()
    };
    let r = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:20160".into(),
                ..Default::default()
            }),
            region: Some(region.clone()),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    let key = format!("/pd/{id}/raft/status/raft_bootstrap_time");
    let read = Read::range(key.as_bytes(), b"", Consistency::Linearizable);
    let persisted = store.read(read.clone()).await.unwrap();
    let stamp = astersql_pd_typeutil::parse_timestamp(&persisted.entries[0].value).unwrap();
    let after = request(url, "GET", "/pd/api/v1/cluster/status", "");
    assert_eq!(after.0, 200);
    assert_eq!(after.1["is_initialized"], false);
    let parsed =
        chrono::DateTime::parse_from_rfc3339(after.1["raft_bootstrap_time"].as_str().unwrap())
            .unwrap();
    assert_eq!(std::time::SystemTime::from(parsed), stamp);
    assert_eq!(after.1["replication_status"], "");
    // Real heartbeat publication controls initialization, including pending voters.
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let mut heartbeat_client = client.clone();
    let stream_task = tokio::spawn(async move {
        let mut stream = heartbeat_client
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner();
        while stream.message().await.unwrap().is_some() {}
    });
    region.peers.extend([
        metapb::Peer {
            id: 4,
            store_id: 2,
            ..Default::default()
        },
        metapb::Peer {
            id: 5,
            store_id: 3,
            ..Default::default()
        },
    ]);
    let mut observations = Vec::new();
    for pending in [false, true, false] {
        region.region_epoch.as_mut().unwrap().conf_ver += 1;
        tx.send(pdpb::RegionHeartbeatRequest {
            header: header.clone(),
            region: Some(region.clone()),
            leader: Some(region.peers[0].clone()),
            pending_peers: if pending {
                vec![region.peers[1].clone()]
            } else {
                vec![]
            },
            ..Default::default()
        })
        .await
        .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let r = request(url, "POST", "/pd/api/v1/cluster/status", "");
            if r.1["is_initialized"] == !pending {
                observations.push(r.1["is_initialized"].clone());
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "{r:?}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    assert_eq!(persisted.entries, store.read(read).await.unwrap().entries);
    stream_task.abort();
    let _ = stream_task.await;
    // Invalid persisted time must surface the Go decoder error, never a fixed status.
    store
        .transact(vec![], vec![Mutation::put(key.as_bytes(), b"bad")])
        .await
        .unwrap();
    let bad = request(url, "GET", "/pd/api/v1/cluster/status", "");
    assert_eq!(
        bad,
        (
            500,
            json!("[PD:typeutil:ErrBytesToUint64]invalid data, must 8 bytes, but 3")
        )
    );
    store
        .transact(
            vec![],
            vec![Mutation::put(key.as_bytes(), &persisted.entries[0].value)],
        )
        .await
        .unwrap();
    assert_eq!(request(url, "GET", "/pd/api/v1/status", ""), status);
    json!({"before":before,"replication_status":after.1["replication_status"],"initialized":observations,"bad":bad,"version_keys":version.1.as_object().unwrap().keys().collect::<Vec<_>>(),"status_keys":status.1.as_object().unwrap().keys().collect::<Vec<_>>()})
}
async fn rust_observe() -> Value {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 746,
                member_id: 1,
                name: "cluster".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let result = observe(&url, store.as_ref(), 746).await;
    node.shutdown().await.unwrap();
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_046_behavior() {
    rust_observe().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_046_differential() {
    let rust = rust_observe().await;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(5))
            .await
            .unwrap();
    let result = observe(&go.url, &store, id).await;
    go.stop().unwrap();
    println!("Go cluster observations: {result}\nRust cluster observations: {rust}");
    assert_eq!(result, rust);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_046_configuration_recovery_and_follower() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let paths = astersql_pd_keypath::ClusterPaths::new(7462);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    endpoint.save_config(&paths,&json!({"replication":{"max-replicas":5,"enable-placement-rules":"false"},"pd-server":{"key-type":"raw","use-region-storage":"false"}})).await.unwrap();
    endpoint
        .save_meta(
            &paths,
            &astersql_pd_storage_endpoint::Cluster {
                id: 7462,
                max_peer_count: 9,
            },
        )
        .await
        .unwrap();
    let timestamp = 1_700_000_000_120_000_000_u64;
    store
        .transact(
            vec![],
            vec![Mutation::put(
                paths.cluster_bootstrap_time_path().as_bytes(),
                &timestamp.to_be_bytes(),
            )],
        )
        .await
        .unwrap();
    for id in [1, 2] {
        endpoint
            .save_region(
                &paths,
                &astersql_pd_core::RegionMeta {
                    id,
                    key_range: astersql_pd_core::KeyRange {
                        start_key: if id == 1 { vec![] } else { b"m".to_vec() },
                        end_key: if id == 1 { b"m".to_vec() } else { vec![] },
                    },
                    epoch: Some(astersql_pd_core::RegionEpoch {
                        conf_ver: 1,
                        version: 1,
                    }),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    let mut addresses = Vec::new();
    for _ in 0..2 {
        let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        addresses.push(reservation.local_addr().unwrap());
    }
    let mut expected = None;
    for _ in 0..2 {
        let mut nodes = Vec::new();
        for id in 1..=2 {
            let listener = tokio::net::TcpListener::bind(addresses[(id - 1) as usize])
                .await
                .unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let mut node =
                PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                    .start(
                        store.clone(),
                        NodeConfig {
                            cluster_id: 7462,
                            member_id: id,
                            name: format!("cluster-{id}"),
                            client_url: url.clone(),
                            peer_url: format!("http://peer-{id}"),
                        },
                        listener,
                        Arc::new(prometheus::Registry::new()),
                    )
                    .await
                    .unwrap();
            node.ready().await.unwrap();
            assert_eq!(
                request(&url, "GET", "/pd/api/v1/cluster", ""),
                (200, json!({"id":7462,"max_peer_count":5}))
            );
            let status = request(&url, "GET", "/pd/api/v1/cluster/status", "");
            assert_eq!(
                status,
                (
                    200,
                    json!({"raft_bootstrap_time":"2023-11-14T22:13:20.12Z","is_initialized":true,"replication_status":""})
                )
            );
            if let Some(prior) = &expected {
                assert_eq!(&status, prior);
            } else {
                expected = Some(status);
            }
            nodes.push(node);
        }
        for node in nodes.iter_mut().rev() {
            node.shutdown().await.unwrap();
        }
    }
}
