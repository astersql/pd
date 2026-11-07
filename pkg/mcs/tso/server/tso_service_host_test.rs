// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::KeyspaceGroupManager;
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use astersql_pd_tso::GroupWatchConfig;
use prost::Message;
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../../../storage/kv/etcd_fixture_test.rs"]
mod fixture;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_082_ready_registry_and_shutdown() {
    let f = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let cluster = 820;
    let groups = Arc::new(KeyspaceGroupManager::new(
        store.clone(),
        ClusterPaths::new(cluster),
    ));
    groups.bootstrap().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let address = format!("http://{addr}");
    let cancel = tokio_util::sync::CancellationToken::new();
    let config = GroupWatchConfig {
        store: store.clone(),
        runtime: tokio::runtime::Handle::current(),
        cancel: cancel.clone(),
        cluster_id: cluster,
        address: address.clone(),
        bootstrap_keyspace: 0,
        participant: Arc::new({
            let address = address.clone();
            move |id| {
                astersql_pd_proto::tsopb::Participant {
                    name: format!("host-{id}"),
                    listen_urls: vec![address.clone()],
                    ..Default::default()
                }
                .encode_to_vec()
            }
        }),
        clock: Arc::new(astersql_pd_tso::SystemClock),
        lease_seconds: 3,
        save_interval_millis: 1000,
    };
    let host = TsoServiceHost::start(config.clone(), groups.clone(), listener)
        .await
        .unwrap();
    assert!(host.is_ready());
    let duplicate_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let duplicate_addr = duplicate_listener.local_addr().unwrap();
    assert!(matches!(
        TsoServiceHost::start(config.clone(), groups.clone(), duplicate_listener).await,
        Err(astersql_pd_tso::TsoError::Member(
            astersql_pd_election::ElectionError::Contended
        ))
    ));
    assert!(
        tokio::net::TcpStream::connect(duplicate_addr)
            .await
            .is_err()
    );
    assert!(
        host.is_ready(),
        "duplicate registration must not cancel the first host"
    );
    let wrong = Arc::new(KeyspaceGroupManager::new(
        store.clone(),
        ClusterPaths::new(cluster + 1),
    ));
    let extra = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    assert!(matches!(
        TsoServiceHost::start(config.clone(), wrong, extra).await,
        Err(astersql_pd_tso::TsoError::StoreMismatch)
    ));

    let key = ClusterPaths::new(cluster).registry_path("tso", &address);
    let entries = store
        .read(Read::prefix(key.as_bytes(), Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    assert_eq!(entries.len(), 1);
    assert!(entries[0].lease.is_some());
    let mut health =
        astersql_pd_proto::health::health_client::HealthClient::connect(address.clone())
            .await
            .unwrap();
    let mut health_watch = health
        .watch(astersql_pd_proto::health::HealthCheckRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(health_watch.message().await.unwrap().unwrap().status, 1);
    let mut client = astersql_pd_proto::tsopb::tso_client::TsoClient::connect(address)
        .await
        .unwrap();
    let header = Some(astersql_pd_proto::tsopb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let timestamp = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let stream = client
                .tso(tokio_stream::iter([astersql_pd_proto::tsopb::TsoRequest {
                    header: header.clone(),
                    count: 3,
                    ..Default::default()
                }]))
                .await;
            if let Ok(stream) = stream {
                let mut stream = stream.into_inner();
                if let Ok(Some(response)) = stream.message().await {
                    assert_eq!(response.count, 3);
                    assert!(response.timestamp.as_ref().unwrap().physical > 0);
                    assert!(stream.message().await.unwrap().is_none());
                    break response;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let discovery = client
        .find_group_by_keyspace_id(astersql_pd_proto::tsopb::FindGroupByKeyspaceIdRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(discovery.header.as_ref().unwrap().error.is_none());
    assert_eq!(
        discovery
            .keyspace_group
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter(|m| m.is_primary)
            .count(),
        1
    );
    let response = client
        .get_min_ts(astersql_pd_proto::tsopb::GetMinTsRequest {
            header: Some(astersql_pd_proto::tsopb::RequestHeader {
                cluster_id: cluster,
                keyspace_group_id: 9,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.keyspace_groups_total, 1);
    assert_eq!(response.header.as_ref().unwrap().keyspace_group_id, 9);
    assert_eq!(response.keyspace_groups_serving, 1);
    assert!(
        response.timestamp.as_ref().unwrap().physical
            >= timestamp.timestamp.as_ref().unwrap().physical
    );
    if let Some(path) = std::env::var_os("PD_SA016_EVIDENCE") {
        std::fs::write(std::path::PathBuf::from(path).with_extension("tso-embedded.json"),serde_json::to_vec(&serde_json::json!({"tso":timestamp.encode_to_vec(),"discovery":discovery.encode_to_vec(),"minimum":response.encode_to_vec(),"boundary":"actual native backend, real TsoServiceHost on caller runtime, canonical allocator and protobuf participant identity; subsequent original retained-stream shutdown and caller ownership assertions remain"})).unwrap()).unwrap();
    }

    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let mut open_stream = client
        .tso(tokio_stream::wrappers::ReceiverStream::new(receiver))
        .await
        .unwrap()
        .into_inner();
    tokio::time::timeout(std::time::Duration::from_secs(5), host.shutdown())
        .await
        .unwrap()
        .unwrap();
    let end = tokio::time::timeout(std::time::Duration::from_secs(5), open_stream.message())
        .await
        .unwrap();
    assert!(!matches!(end, Ok(Some(_))));
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), health_watch.message())
            .await
            .unwrap()
            .is_err()
    );
    drop(sender);
    assert!(
        !cancel.is_cancelled(),
        "host shutdown must not cancel its caller"
    );
    assert!(
        store
            .read(Read::prefix(key.as_bytes(), Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
}
