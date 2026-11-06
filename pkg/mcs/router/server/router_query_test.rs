// Copyright 2026 AsterSQL.
use super::{HostConfig, RouterHost, Service};
use astersql_pd_core::{BasicCluster, Peer, RegionEpoch, RegionInfo, RegionMeta};
use astersql_pd_mcs_discovery::ServiceRegistryEntry;
use astersql_pd_mcs_server::DelegateConnections;
use astersql_pd_proto::{
    metapb, pdpb,
    routerpb::{router_client::RouterClient, router_server::Router},
};
use astersql_pd_storage_domain::{MemoryStore, MetaStore, Mutation};
use prost::Message;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
fn region(id: u64, start: &[u8], end: &[u8]) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: astersql_pd_core::KeyRange {
                start_key: start.into(),
                end_key: end.into(),
            },
            epoch: Some(RegionEpoch {
                conf_ver: 1,
                version: 1,
            }),
            ..Default::default()
        },
        Some(Peer {
            id: id + 100,
            store_id: 1,
            ..Default::default()
        }),
        vec![],
        vec![],
    ))
}
#[tokio::test]
async fn mcs_086_same_canonical_cache_point_previous_id_and_live_update() {
    let cluster = Arc::new(Mutex::new(BasicCluster::new()));
    cluster
        .lock()
        .unwrap()
        .regions
        .put_region(region(1, b"", b"m"));
    cluster
        .lock()
        .unwrap()
        .regions
        .put_region(region(2, b"m", b""));
    let service = Service::new(91, cluster.clone(), CancellationToken::new());
    let request = pdpb::GetRegionRequest {
        region_key: b"m".to_vec(),
        ..Default::default()
    };
    let point = service
        .get_region(tonic::Request::new(request.clone()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(point.region.unwrap().id, 2);
    assert_eq!(point.header.unwrap().cluster_id, 91);
    let previous = service
        .get_prev_region(tonic::Request::new(request))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(previous.region.unwrap().id, 1);
    let id = service
        .get_region_by_id(tonic::Request::new(pdpb::GetRegionByIdRequest {
            region_id: 2,
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(id.region.unwrap().start_key, b"m");
    cluster
        .lock()
        .unwrap()
        .regions
        .put_region(region(3, b"", b""));
    let replaced = service
        .get_region(tonic::Request::new(pdpb::GetRegionRequest::default()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(replaced.region.unwrap().id, 3);
    let missing = service
        .get_region_by_id(tonic::Request::new(pdpb::GetRegionByIdRequest {
            region_id: 2,
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(missing.region.is_none());
    assert!(missing.header.unwrap().error.is_none());
}
#[tokio::test]
async fn mcs_087_batch_range_validation_holes_and_empty_leader_presence() {
    let cluster = Arc::new(Mutex::new(BasicCluster::new()));
    let r = RegionInfo::new(region(1, b"a", b"m").meta().clone(), None, vec![], vec![]);
    cluster.lock().unwrap().regions.put_region(Arc::new(r));
    let service = Service::new(91, cluster, CancellationToken::new());
    for (ranges, expected) in [
        (
            vec![(b"z".as_slice(), b"a".as_slice())],
            "invalid key range, start key > end key",
        ),
        (
            vec![
                (b"a".as_slice(), b"m".as_slice()),
                (b"b".as_slice(), b"z".as_slice()),
            ],
            "invalid key range, ranges overlapped",
        ),
    ] {
        let response = service
            .batch_scan_regions(tonic::Request::new(pdpb::BatchScanRegionsRequest {
                ranges: ranges
                    .into_iter()
                    .map(|(s, e)| pdpb::KeyRange {
                        start_key: s.to_vec(),
                        end_key: e.to_vec(),
                    })
                    .collect(),
                ..Default::default()
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(response.header.unwrap().error.unwrap().message, expected);
    }
    let response = service
        .batch_scan_regions(tonic::Request::new(pdpb::BatchScanRegionsRequest {
            ranges: vec![pdpb::KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"m".to_vec(),
            }],
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.regions.len(), 1);
    assert_eq!(response.regions[0].leader, Some(metapb::Peer::default()));
    let hole = service
        .batch_scan_regions(tonic::Request::new(pdpb::BatchScanRegionsRequest {
            ranges: vec![pdpb::KeyRange {
                start_key: b"0".to_vec(),
                end_key: b"z".to_vec(),
            }],
            contain_all_key_range: true,
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        hole.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::RegionsNotContainAllKeyRange as i32
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcs_137_embedding_observes_store_metadata_and_shutdown_closes_retained_client() {
    let store = Arc::new(MemoryStore::default());
    let paths = astersql_pd_keypath::ClusterPaths::new(91);
    let key = paths.store_path(1);
    store
        .transact(
            vec![],
            vec![Mutation::put(
                key.as_bytes(),
                &metapb::Store {
                    id: 1,
                    address: "before".into(),
                    ..Default::default()
                }
                .encode_to_vec(),
            )],
        )
        .await
        .unwrap();
    let cluster = Arc::new(Mutex::new(BasicCluster::new()));
    cluster
        .lock()
        .unwrap()
        .regions
        .put_region(region(1, b"", b""));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let runtime = tokio::runtime::Handle::current();
    let delegates = Arc::new(DelegateConnections::new(runtime.clone(), &cancel, None));
    let host = RouterHost::start(
        HostConfig {
            runtime,
            cancel: cancel.clone(),
            store: store.clone(),
            cluster_id: 91,
            entry: ServiceRegistryEntry {
                name: "embedded-router".into(),
                service_addr: url.clone(),
                ..Default::default()
            },
            cluster,
            delegates: delegates.clone(),
        },
        listener,
    )
    .await
    .unwrap();
    let mut client = RouterClient::connect(url).await.unwrap();
    let request = pdpb::GetStoreRequest {
        store_id: 1,
        ..Default::default()
    };
    assert_eq!(
        client
            .get_store(request.clone())
            .await
            .unwrap()
            .into_inner()
            .store
            .unwrap()
            .address,
        "before"
    );
    store
        .transact(
            vec![],
            vec![Mutation::put(
                key.as_bytes(),
                &metapb::Store {
                    id: 1,
                    address: "after".into(),
                    ..Default::default()
                }
                .encode_to_vec(),
            )],
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while client
            .get_store(request.clone())
            .await
            .unwrap()
            .into_inner()
            .store
            .unwrap()
            .address
            != "after"
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: key.into_bytes(),
            }],
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while client
            .get_store(request.clone())
            .await
            .unwrap()
            .into_inner()
            .store
            .is_some()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    host.shutdown().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), client.get_store(request))
            .await
            .unwrap()
            .is_err()
    );
    delegates.shutdown();
}
