// Copyright 2026 AsterSQL.
use crate::{accept_extra_rm_service_test::Sidecar, harness::Server};
use astersql_pd_proto::{metapb, pdpb, routerpb::router_client::RouterClient};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_router_standalone_registered_methods_and_query_eof() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    let mut expected = None;
    let mut expected_populated = None;
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut pd = Server::start(&go).unwrap();
        pd.ready().unwrap();
        let mut identity = pdpb::pd_client::PdClient::connect(pd.url.clone())
            .await
            .unwrap();
        let cluster = identity
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let mut sidecar = Sidecar::start_service(binary, pd.metadata_url(), "router", "http");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut client = loop {
            if let Some(status) = sidecar.child.try_wait().unwrap() {
                records.push(json!({"rust":is_rust,"stage":"startup","exit":status.to_string(),"log":sidecar.logs()}));
                save(&records);
                panic!(
                    "Router process must serve its registered methods: {status}: {}",
                    sidecar.logs()
                );
            }
            if let Ok(mut client) = RouterClient::connect(sidecar.url.clone()).await {
                if client
                    .get_all_stores(pdpb::GetAllStoresRequest {
                        header: Some(pdpb::RequestHeader::default()),
                        ..Default::default()
                    })
                    .await
                    .is_ok()
                {
                    break client;
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "router startup timeout: {}",
                sidecar.logs()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        // Router unary helpers intentionally do not require a PD leader or a
        // matching request cluster ID. Their cache exists even before PD bootstrap.
        let point = client
            .get_region(pdpb::GetRegionRequest {
                header: Some(pdpb::RequestHeader::default()),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let previous = client
            .get_prev_region(pdpb::GetRegionRequest {
                header: Some(pdpb::RequestHeader::default()),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let id = client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: Some(pdpb::RequestHeader::default()),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let scan = client
            .batch_scan_regions(pdpb::BatchScanRegionsRequest {
                header: Some(pdpb::RequestHeader::default()),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let missing = client
            .get_store(pdpb::GetStoreRequest {
                store_id: 17,
                header: Some(pdpb::RequestHeader::default()),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let all = client
            .get_all_stores(pdpb::GetAllStoresRequest {
                header: Some(pdpb::RequestHeader::default()),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let (send, receive) = tokio::sync::mpsc::channel(1);
        send.send(pdpb::QueryRegionRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            ids: vec![17],
            ..Default::default()
        })
        .await
        .unwrap();
        drop(send);
        let mut stream = client
            .query_region(tokio_stream::wrappers::ReceiverStream::new(receive))
            .await
            .unwrap()
            .into_inner();
        let query = stream.message().await.unwrap().unwrap();
        assert!(stream.message().await.unwrap().is_none());
        for header in [
            &point.header,
            &previous.header,
            &id.header,
            &scan.header,
            &all.header,
            &query.header,
        ] {
            assert_eq!(header.as_ref().unwrap().cluster_id, cluster);
            assert!(header.as_ref().unwrap().error.is_none());
        }
        assert_eq!(
            missing
                .header
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .message,
            "invalid store ID 17, not found"
        );
        let invalid_request = pdpb::QueryRegionRequest {
            header: Some(pdpb::RequestHeader::default()),
            ..Default::default()
        };
        let invalid = match client
            .query_region(tokio_stream::iter([invalid_request]))
            .await
        {
            Ok(response) => response.into_inner().message().await.unwrap_err(),
            Err(error) => error,
        };
        records.push(json!({"rust":is_rust,"stage":"query wrong cluster","code":format!("{:?}",invalid.code()),"message":invalid.message()}));
        save(&records);
        let observed = json!({"query_wrong_cluster": [format!("{:?}",invalid.code()), invalid.message().replace(&cluster.to_string(), "<cluster>")],"point":format!("{:?}",point.region),"prev":format!("{:?}",previous.region),"id":format!("{:?}",id.region),"scan":format!("{:?}",scan.regions),"store":format!("{:?}",missing.header.unwrap().error),"all":format!("{:?}",all.stores),"query":format!("{:?}",query.regions_by_id)});
        records.push(
            json!({"rust":is_rust,"stage":"registered methods and EOF","responses":observed}),
        );
        save(&records);
        if let Some(expected) = expected.as_ref() {
            assert_eq!(&observed, expected);
        } else {
            expected = Some(observed);
        }
        let header = Some(pdpb::RequestHeader {
            cluster_id: cluster,
            ..Default::default()
        });
        let bootstrap = identity
            .bootstrap(pdpb::BootstrapRequest {
                header: header.clone(),
                store: Some(metapb::Store {
                    id: 1,
                    address: "router-source".into(),
                    ..Default::default()
                }),
                region: Some(metapb::Region {
                    id: 2,
                    peers: vec![metapb::Peer {
                        id: 3,
                        store_id: 1,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(bootstrap.header.unwrap().error.is_none());
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let row = client
                    .get_region_by_id(pdpb::GetRegionByIdRequest {
                        header: header.clone(),
                        region_id: 2,
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner();
                if row.region.is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        let mut point = client
            .get_region(pdpb::GetRegionRequest {
                header: header.clone(),
                region_key: b"foo".to_vec(),
                need_buckets: true,
            })
            .await
            .unwrap()
            .into_inner();
        let mut previous = client
            .get_prev_region(pdpb::GetRegionRequest {
                header: header.clone(),
                region_key: b"foo".to_vec(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let mut by_id = client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: header.clone(),
                region_id: 2,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let mut scan = client
            .batch_scan_regions(pdpb::BatchScanRegionsRequest {
                header: header.clone(),
                ranges: vec![pdpb::KeyRange::default()],
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let mut store = client
            .get_store(pdpb::GetStoreRequest {
                header: header.clone(),
                store_id: 1,
            })
            .await
            .unwrap()
            .into_inner();
        let mut all = client
            .get_all_stores(pdpb::GetAllStoresRequest {
                header: header.clone(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let (send, receive) = tokio::sync::mpsc::channel(1);
        send.send(pdpb::QueryRegionRequest {
            header: header.clone(),
            keys: vec![b"foo".to_vec()],
            prev_keys: vec![b"foo".to_vec()],
            ids: vec![2, 17],
            ..Default::default()
        })
        .await
        .unwrap();
        drop(send);
        let mut stream = client
            .query_region(tokio_stream::wrappers::ReceiverStream::new(receive))
            .await
            .unwrap()
            .into_inner();
        let mut query = stream.message().await.unwrap().unwrap();
        assert!(stream.message().await.unwrap().is_none());
        use prost::Message;
        records.push(json!({"rust":is_rust,"stage":"populated raw wire","point":point.encode_to_vec(),"previous":previous.encode_to_vec(),"by_id":by_id.encode_to_vec(),"scan":scan.encode_to_vec(),"store":store.encode_to_vec(),"all":all.encode_to_vec(),"query":query.encode_to_vec()}));
        save(&records);
        assert_eq!(point.region.as_ref().unwrap().id, 2);
        assert_eq!(by_id.region.as_ref().unwrap().id, 2);
        assert_eq!(scan.regions.len(), 1);
        assert_eq!(store.store.as_ref().unwrap().id, 1);
        for h in [
            &mut point.header,
            &mut previous.header,
            &mut by_id.header,
            &mut scan.header,
            &mut store.header,
            &mut all.header,
            &mut query.header,
        ] {
            h.as_mut().unwrap().cluster_id = 0;
        }
        let sorted: std::collections::BTreeMap<_, _> = query.regions_by_id.iter().collect();
        let populated = json!({"point":format!("{point:?}"),"previous":format!("{previous:?}"),"id":format!("{by_id:?}"),"scan":format!("{scan:?}"),"store":format!("{store:?}"),"all":format!("{all:?}"),"query_header":format!("{:?}",query.header),"keys":query.key_id_map,"prev":query.prev_key_id_map,"query":format!("{sorted:?}")});
        records.push(json!({"rust":is_rust,"stage":"populated comparison","responses":populated}));
        save(&records);
        if let Some(expected) = expected_populated.as_ref() {
            assert_eq!(&populated, expected);
        } else {
            expected_populated = Some(populated);
        }
        sidecar.stop();
        pd.stop().unwrap();
    }
}
fn save(records: &[serde_json::Value]) {
    let path =
        PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap()).with_extension("router.json");
    std::fs::write(path, serde_json::to_vec(records).unwrap()).unwrap();
}
