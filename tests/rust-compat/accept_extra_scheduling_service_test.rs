// Copyright 2026 AsterSQL.
use crate::{accept_extra_rm_service_test::Sidecar, harness::Server};
use astersql_pd_proto::{
    metapb, pdpb,
    schedulingpb::{self as p, scheduling_client::SchedulingClient},
};
use prost::Message;
use serde_json::json;
use std::{path::PathBuf, time::Duration};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_standalone_seven_methods_and_empty_stream_eof() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    let mut expected = None;
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut pd = Server::start(&go).unwrap();
        pd.ready().unwrap();
        let mut client = pdpb::pd_client::PdClient::connect(pd.url.clone())
            .await
            .unwrap();
        let cluster = client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        assert!(
            client
                .bootstrap(pdpb::BootstrapRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: cluster,
                        ..Default::default()
                    }),
                    store: Some(metapb::Store {
                        id: 1,
                        address: "scheduling-source".into(),
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
                    })
                })
                .await
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .error
                .is_none()
        );
        let mut sidecar = Sidecar::start_service(binary, pd.metadata_url(), "scheduling", "http");
        let header = Some(p::RequestHeader {
            cluster_id: cluster,
            sender_id: 0,
        });
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let mut client = loop {
            if let Some(status) = sidecar.child.try_wait().unwrap() {
                records.push(json!({"rust":is_rust,"stage":"startup","exit":status.to_string(),"log":sidecar.logs()}));
                save(&records);
                panic!(
                    "Scheduling must serve its registered methods: {status}: {}",
                    sidecar.logs()
                );
            }
            if let Ok(mut client) = SchedulingClient::connect(sidecar.url.clone()).await {
                if let Ok(response) = client
                    .get_operator(p::GetOperatorRequest {
                        header: header.clone(),
                        region_id: 17,
                    })
                    .await
                {
                    if response
                        .into_inner()
                        .header
                        .is_some_and(|h| h.error.is_some_and(|e| e.message == "region not found"))
                    {
                        break client;
                    }
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "Scheduling primary not ready: {}",
                sidecar.logs()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        let mut store = client
            .store_heartbeat(p::StoreHeartbeatRequest {
                header: header.clone(),
                stats: Some(pdpb::StoreStats {
                    store_id: 17,
                    ..Default::default()
                }),
            })
            .await
            .unwrap()
            .into_inner();
        let mut operator = client
            .get_operator(p::GetOperatorRequest {
                header: header.clone(),
                region_id: 17,
            })
            .await
            .unwrap()
            .into_inner();
        let mut split = client
            .split_regions(p::SplitRegionsRequest {
                header: header.clone(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let mut scatter = client
            .scatter_regions(p::ScatterRegionsRequest {
                header: header.clone(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let mut ask = client
            .ask_batch_split(p::AskBatchSplitRequest {
                header: header.clone(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let empty: Vec<p::RegionHeartbeatRequest> = vec![];
        let heartbeat = terminal(client.region_heartbeat(tokio_stream::iter(empty)).await).await;
        let buckets = terminal(
            client
                .region_buckets(tokio_stream::iter([p::RegionBucketsRequest {
                    header: header.clone(),
                    buckets: None,
                    ..Default::default()
                }]))
                .await,
        )
        .await;
        records.push(json!({"rust":is_rust,"stage":"raw wire and stream EOF","store":store.encode_to_vec(),"operator":operator.encode_to_vec(),"split":split.encode_to_vec(),"scatter":scatter.encode_to_vec(),"ask":ask.encode_to_vec(),"heartbeat_terminal":heartbeat,"buckets_terminal":buckets}));
        save(&records);
        for h in [
            &mut store.header,
            &mut operator.header,
            &mut split.header,
            &mut scatter.header,
            &mut ask.header,
        ] {
            h.as_mut().unwrap().cluster_id = 0;
        }
        let observed = json!({"heartbeat_terminal":heartbeat,"buckets_terminal":buckets,"store":format!("{store:?}"),"operator":format!("{operator:?}"),"split":format!("{split:?}"),"scatter":format!("{scatter:?}"),"ask":format!("{ask:?}")});
        records.push(json!({"rust":is_rust,"stage":"comparison","responses":observed}));
        save(&records);
        if let Some(expected) = expected.as_ref() {
            assert_eq!(&observed, expected);
        } else {
            expected = Some(observed);
        }
        sidecar.stop();
        pd.stop().unwrap();
    }
}
fn save(records: &[serde_json::Value]) {
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("scheduling.json");
    std::fs::write(path, serde_json::to_vec(records).unwrap()).unwrap();
}

async fn terminal<T: std::fmt::Debug + Send + 'static>(
    result: Result<tonic::Response<tonic::Streaming<T>>, tonic::Status>,
) -> serde_json::Value {
    let status = match result {
        Err(status) => status,
        Ok(response) => match response.into_inner().message().await {
            Ok(None) => return json!({"terminal":"EOF"}),
            Ok(Some(frame)) => {
                panic!("empty/skipped heartbeat stream must not emit a data frame: {frame:?}")
            }
            Err(status) => status,
        },
    };
    json!({"terminal":"status","code":format!("{:?}",status.code()),"message":status.message()})
}

struct ConfigFile(PathBuf);
impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_heartbeats_keep_region_metadata_pd_owned_and_allocate_split_ids_through_pd()
 {
    use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    let mut expected_first_allocation = None;
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let config = ConfigFile(std::env::temp_dir().join(format!(
            "sa016-scheduling-active-{}-{is_rust}.toml",
            std::process::id()
        )));
        std::fs::write(&config.0, "[replication]\nmax-replicas=1\n").unwrap();
        let arg = format!("--config={}", config.0.display());
        let mut pd = Server::start_with_args(&go, &[&arg]).unwrap();
        pd.ready().unwrap();
        let mut owner = pdpb::pd_client::PdClient::connect(pd.url.clone())
            .await
            .unwrap();
        let cluster = owner
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let pd_header = Some(pdpb::RequestHeader {
            cluster_id: cluster,
            ..Default::default()
        });
        assert!(
            owner
                .bootstrap(pdpb::BootstrapRequest {
                    header: pd_header.clone(),
                    store: Some(metapb::Store {
                        id: 1,
                        address: "idle-scheduling-source".into(),
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
                    })
                })
                .await
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .error
                .is_none()
        );
        let backend = astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[pd.metadata_url().into()],
            Duration::from_secs(3),
        )
        .await
        .unwrap();
        let paths = astersql_pd_keypath::ClusterPaths::new(cluster);
        let key = paths.region_path(2);
        let before = backend
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        let mut sidecar = Sidecar::start_service(binary, pd.metadata_url(), "scheduling", "http");
        let header = Some(p::RequestHeader {
            cluster_id: cluster,
            sender_id: 0,
        });
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let mut client =
            loop {
                if let Some(status) = sidecar.child.try_wait().unwrap() {
                    panic!("scheduling startup failed {status}: {}", sidecar.logs());
                }
                if let Ok(mut client) = SchedulingClient::connect(sidecar.url.clone()).await {
                    if let Ok(response) = client
                        .get_operator(p::GetOperatorRequest {
                            header: header.clone(),
                            region_id: 17,
                        })
                        .await
                    {
                        if response.into_inner().header.is_some_and(|h| {
                            h.error.is_some_and(|e| e.message == "region not found")
                        }) {
                            break client;
                        }
                    }
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "scheduling primary not ready: {}",
                    sidecar.logs()
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            };
        let valid_store = p::StoreHeartbeatRequest {
            header: header.clone(),
            stats: Some(pdpb::StoreStats {
                store_id: 1,
                capacity: 100,
                available: 80,
                region_count: 1,
                ..Default::default()
            }),
        };
        let response = client
            .store_heartbeat(valid_store.clone())
            .await
            .unwrap()
            .into_inner();
        records.push(json!({"rust":is_rust,"stage":"valid store heartbeat","request":valid_store.encode_to_vec(),"response":response.encode_to_vec()}));
        save_active(&records);
        assert!(response.header.unwrap().error.is_none());
        let baseline = owner
            .alloc_id(pdpb::AllocIdRequest {
                header: pd_header.clone(),
                count: 1,
            })
            .await
            .unwrap()
            .into_inner()
            .id;
        let peer = metapb::Peer {
            id: 3,
            store_id: 1,
            ..Default::default()
        };
        let region = metapb::Region {
            id: 2,
            region_epoch: Some(metapb::RegionEpoch {
                conf_ver: 1,
                version: 1,
            }),
            peers: vec![peer.clone()],
            ..Default::default()
        };
        let report = p::RegionHeartbeatRequest {
            header: header.clone(),
            region: Some(region.clone()),
            leader: Some(peer),
            approximate_size: 1024 * 1024,
            approximate_keys: 1,
            ..Default::default()
        };
        let invalid = p::RegionHeartbeatRequest {
            header: header.clone(),
            leader: Some(metapb::Peer {
                id: 999,
                store_id: 999,
                ..Default::default()
            }),
            ..Default::default()
        };
        let (send, receive) = tokio::sync::mpsc::channel(2);
        let mut heartbeat_client = client.clone();
        let heartbeat = tokio::spawn(async move {
            heartbeat_client
                .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(receive))
                .await
        });
        send.send(report.clone()).await.unwrap();
        send.send(invalid.clone()).await.unwrap();
        drop(send);
        let terminal = terminal(
            tokio::time::timeout(Duration::from_secs(5), heartbeat)
                .await
                .unwrap()
                .unwrap(),
        )
        .await;
        records.push(json!({"rust":is_rust,"stage":"region admission followed by invalid-store barrier","baseline":baseline,"request":report.encode_to_vec(),"invalid":invalid.encode_to_vec(),"terminal":terminal}));
        save_active(&records);
        assert_eq!(
            terminal,
            json!({"terminal":"status","code":"Unknown","message":"invalid store ID 999, not found"})
        );
        // Allow the real three-second preparation gate and scheduling timers to
        // run. A fully replicated single-store cluster has no ID-consuming work.
        tokio::time::sleep(Duration::from_secs(4)).await;
        let after_idle = owner
            .alloc_id(pdpb::AllocIdRequest {
                header: pd_header.clone(),
                count: 1,
            })
            .await
            .unwrap()
            .into_inner()
            .id;
        let after = backend
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        records.push(json!({"rust":is_rust,"stage":"idle scheduler ownership","before_id":baseline,"after_id":after_idle,"region_before":format!("{:?}",before.entries),"region_after":format!("{:?}",after.entries),"log":sidecar.logs()}));
        save_active(&records);
        assert_eq!(
            after.entries, before.entries,
            "scheduling heartbeats must not persist PD-owned region metadata"
        );
        assert_eq!(
            after_idle,
            baseline + 1,
            "idle scheduling must not consume global IDs before a planner needs them"
        );
        let request = p::AskBatchSplitRequest {
            header: header.clone(),
            region: Some(region),
            split_count: 1,
            ..Default::default()
        };
        let first = client.ask_batch_split(request.clone()).await;
        let first_observation = match &first {
            Ok(_) => json!({"status":"OK"}),
            Err(error) => json!({"status":format!("{:?}",error.code()),"message":error.message()}),
        };
        records.push(
            json!({"rust":is_rust,"stage":"first allocation discovery","result":first_observation}),
        );
        save_active(&records);
        if let Some(expected) = expected_first_allocation.as_ref() {
            assert_eq!(
                &first_observation, expected,
                "initial PD client discovery must match Go"
            );
        } else {
            expected_first_allocation = Some(first_observation);
        }
        let mut pending = first;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let response = loop {
            match pending {
                Ok(response) => break response.into_inner(),
                Err(error)
                    if error.code() == tonic::Code::Unknown
                        && error.message() == "PD leader is not found" =>
                {
                    // This exact Go path returns before sending any allocation RPC.
                    // Never retry a forwarded RPC error with an uncertain outcome.
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "PD client discovery did not complete: {}",
                        sidecar.logs()
                    );
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    pending = client.ask_batch_split(request.clone()).await;
                }
                Err(error) => panic!("split allocation failed; not replaying: {error:?}"),
            }
        };
        records.push(json!({"rust":is_rust,"stage":"PD-owned split allocation","request":request.encode_to_vec(),"response":response.encode_to_vec(),"baseline":after_idle}));
        save_active(&records);
        assert!(response.header.unwrap().error.is_none());
        assert_eq!(response.ids.len(), 1);
        assert_eq!(response.ids[0].new_region_id, after_idle + 1);
        assert_eq!(response.ids[0].new_peer_ids, vec![after_idle + 2]);
        let after_split = owner
            .alloc_id(pdpb::AllocIdRequest {
                header: pd_header,
                count: 1,
            })
            .await
            .unwrap()
            .into_inner()
            .id;
        assert_eq!(
            after_split,
            after_idle + 3,
            "scheduling must share PD's current ID sequence"
        );
        sidecar.stop();
        pd.stop().unwrap();
    }
}
fn save_active(records: &[serde_json::Value]) {
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("scheduling-active.json");
    std::fs::write(path, serde_json::to_vec(records).unwrap()).unwrap();
}
