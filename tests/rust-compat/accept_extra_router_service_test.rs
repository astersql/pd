// Copyright 2026 AsterSQL.
use crate::accept_modes::Deployment;
use astersql_pd_proto::{metapb, pdpb, routerpb::router_client::RouterClient};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_router_standalone_registered_methods_and_query_eof() {
    matrix().await;
}
pub(super) async fn matrix() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    let mut expected = None;
    let mut expected_populated = None;
    let certs = super::accept_extra_transport_config_test::certificates("router-business");
    for (binary, is_rust, embedded, tls) in [
        (&go, false, false, false),
        (&rust, true, false, false),
        (&rust, true, true, false),
        (&go, false, false, true),
        (&rust, true, false, true),
        (&rust, true, true, true),
    ] {
        let cert_path = tls.then_some(certs.0.as_path());
        let mut pd = super::accept_modes::pd_owner(&go, &[], &[], cert_path).await;
        let mut identity = pdpb::pd_client::PdClient::new(
            super::accept_modes::channel(&pd.url, cert_path, Some("allowed"))
                .await
                .unwrap(),
        );
        let cluster = identity
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let argument = cert_path.map(|p| super::accept_modes::tls_argument(p, "router"));
        let extra = argument.iter().map(String::as_str).collect::<Vec<_>>();
        let mut sidecar = Deployment::start(
            binary,
            pd.metadata_url(),
            "router",
            cluster,
            &extra,
            embedded,
            cert_path,
        )
        .await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut client = loop {
            if sidecar.failed().unwrap() {
                let status = "unexpected process exit";
                records.push(json!({"rust":is_rust,"stage":"startup","exit":status.to_string(),"log":sidecar.logs()}));
                save(&records);
                panic!(
                    "Router process must serve its registered methods: {status}: {}",
                    sidecar.logs()
                );
            }
            if let Ok(channel) =
                super::accept_modes::channel(&sidecar.url, cert_path, Some("allowed")).await
            {
                let mut client = RouterClient::new(channel);
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
        records.push(json!({"rust":is_rust,"embedded":embedded,"tls":tls,"stage":"populated raw wire","point":point.encode_to_vec(),"previous":previous.encode_to_vec(),"by_id":by_id.encode_to_vec(),"scan":scan.encode_to_vec(),"store":store.encode_to_vec(),"all":all.encode_to_vec(),"query":query.encode_to_vec()}));
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
        // Dropping a live response cancels only that request; the same genuine cache
        // must remain usable by a new stream, which then half-closes normally.
        for phase in ["cancel live populated query", "reopen populated query"] {
            let (sender, receiver) = tokio::sync::mpsc::channel(1);
            sender
                .send(pdpb::QueryRegionRequest {
                    header: header.clone(),
                    keys: vec![b"foo".to_vec()],
                    prev_keys: vec![b"foo".to_vec()],
                    ids: vec![2, 17],
                    ..Default::default()
                })
                .await
                .unwrap();
            let mut output = client
                .query_region(tokio_stream::wrappers::ReceiverStream::new(receiver))
                .await
                .unwrap()
                .into_inner();
            let mut response = tokio::time::timeout(Duration::from_secs(5), output.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let raw = response.encode_to_vec();
            response.header.as_mut().unwrap().cluster_id = 0;
            assert_eq!(response, query, "{phase}");
            drop(sender);
            if phase.starts_with("reopen") {
                assert!(
                    tokio::time::timeout(Duration::from_secs(5), output.message())
                        .await
                        .unwrap()
                        .unwrap()
                        .is_none()
                );
            }
            drop(output);
            records.push(json!({"rust":is_rust,"stage":phase,"response_raw":raw,"same_shared_cache_response":true,"normal_eof":phase.starts_with("reopen")}));
            save(&records);
        }
        if let Some(certs) = cert_path {
            for identity in [None, Some("denied"), Some("rogue")] {
                let response =
                    match super::accept_modes::channel(&sidecar.url, Some(certs), identity).await {
                        Ok(channel) => RouterClient::new(channel)
                            .get_all_stores(pdpb::GetAllStoresRequest::default())
                            .await
                            .map(|r| format!("{:?}", r.into_inner()))
                            .map_err(|e| e.to_string()),
                        Err(e) => Err(e),
                    };
                assert!(response.is_err());
                records.push(json!({"rust":is_rust,"embedded":embedded,"tls":true,"stage":"router business TLS refusal","identity":identity,"raw":format!("{response:?}")}));
                save(&records);
            }
        }
        sidecar.stop().await;
        pd.stop().unwrap();
    }
}
fn save(records: &[serde_json::Value]) {
    let path =
        PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap()).with_extension("router.json");
    std::fs::write(path, serde_json::to_vec(records).unwrap()).unwrap();
}
