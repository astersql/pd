// Copyright 2026 AsterSQL.
use super::{accept_extra_rm_service_test::Sidecar, accept_extra_transport_config_test::Work};
use astersql_pd_proto::{metapb, pdpb, schedulingpb as p};
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use prost::Message;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn save(records: &[Value], profile: &str) {
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension(format!("scheduling-{profile}.json"));
    std::fs::write(path, serde_json::to_vec(records).unwrap()).unwrap();
}
async fn scenario(profile: &str) {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let work = Work(std::env::temp_dir().join(format!(
        "pd-scheduling-operator-{profile}-{}",
        std::process::id()
    )));
    std::fs::create_dir(&work.0).unwrap();
    let config = work.0.join("pd.toml");
    std::fs::write(
        &config,
        format!(
            "{}[replication]\nmax-replicas=2\nenable-placement-rules=false\n",
            if profile == "replica-v2" {
                "cluster-version={Major=5,Minor=0,Patch=0}\n"
            } else {
                ""
            }
        ),
    )
    .unwrap();
    let arg = format!("--config={}", config.display());
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut owner = crate::harness::Server::start_with_environment(
            &go,
            &[&arg],
            &[("PD_SERVICE_MODE".into(), "api".into())],
        )
        .unwrap();
        owner.ready().unwrap();
        let configuration = owner.get("/pd/api/v1/config").unwrap();
        assert_eq!(configuration.1["replication"]["max-replicas"], 2);
        assert_eq!(
            configuration.1["replication"]["enable-placement-rules"],
            "false"
        );
        let mut pd = pdpb::pd_client::PdClient::connect(owner.url.clone())
            .await
            .unwrap();
        let cluster = pd
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
        let leader = metapb::Peer {
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
            peers: vec![leader.clone()],
            ..Default::default()
        };
        let store1 = metapb::Store {
            id: 1,
            address: "operator-store-one".into(),
            version: if profile == "replica-v2" {
                "5.0.0".into()
            } else {
                String::new()
            },
            node_state: metapb::NodeState::Serving as i32,
            ..Default::default()
        };
        assert!(
            pd.bootstrap(pdpb::BootstrapRequest {
                header: pd_header.clone(),
                store: Some(store1),
                region: Some(region.clone())
            })
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_none()
        );
        assert!(
            pd.put_store(pdpb::PutStoreRequest {
                header: pd_header.clone(),
                store: Some(metapb::Store {
                    id: 2,
                    address: "operator-store-two".into(),
                    version: if profile == "replica-v2" {
                        "5.0.0".into()
                    } else {
                        String::new()
                    },
                    node_state: metapb::NodeState::Serving as i32,
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
        let backend = EtcdMetaStore::connect(&[owner.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
        let region_key = astersql_pd_keypath::ClusterPaths::new(cluster).region_path(2);
        let before = backend
            .read(Read::range(
                region_key.as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        let baseline = pd
            .alloc_id(pdpb::AllocIdRequest {
                header: pd_header.clone(),
                count: 1,
            })
            .await
            .unwrap()
            .into_inner()
            .id;
        let mut sidecar = Sidecar::start_service(binary, &owner.url, "scheduling", "http");
        let header = Some(p::RequestHeader {
            cluster_id: cluster,
            sender_id: 0,
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut client = loop {
            assert!(
                sidecar.child.try_wait().unwrap().is_none(),
                "{}",
                sidecar.logs()
            );
            if let Ok(mut client) =
                p::scheduling_client::SchedulingClient::connect(sidecar.url.clone()).await
            {
                if client
                    .get_operator(p::GetOperatorRequest {
                        header: header.clone(),
                        region_id: 999,
                    })
                    .await
                    .is_ok()
                {
                    break client;
                }
            }
            assert!(
                Instant::now() < deadline,
                "operator service startup: {}",
                sidecar.logs()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        for id in [1, 2] {
            let deadline = Instant::now() + Duration::from_secs(12);
            loop {
                let response = client
                    .store_heartbeat(p::StoreHeartbeatRequest {
                        header: header.clone(),
                        stats: Some(pdpb::StoreStats {
                            store_id: id,
                            capacity: 100 * 1024 * 1024 * 1024,
                            available: 90 * 1024 * 1024 * 1024,
                            region_count: if id == 1 { 1 } else { 0 },
                            ..Default::default()
                        }),
                    })
                    .await;
                if response.is_ok_and(|response| {
                    response
                        .into_inner()
                        .header
                        .is_some_and(|h| h.error.is_none())
                }) {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "store metadata unavailable: {}",
                    sidecar.logs()
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
        let mut reported = region;
        if profile.starts_with("replica") {
            reported.start_key = b"a".to_vec();
            reported.end_key = b"b".to_vec();
            reported.region_epoch.as_mut().unwrap().version = 2;
        }
        let request = p::RegionHeartbeatRequest {
            header: header.clone(),
            region: Some(reported),
            leader: Some(leader),
            approximate_size: 1024 * 1024,
            approximate_keys: 1,
            ..Default::default()
        };
        let (sender, receiver) = tokio::sync::mpsc::channel(4);
        sender.send(request.clone()).await.unwrap();
        let mut stream_client = client.clone();
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let mut stream = stream_client
                .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(receiver))
                .await
                .map_err(|e| e.to_string())?
                .into_inner();
            let response = stream
                .message()
                .await
                .map_err(|e| e.to_string())?
                .ok_or("unexpected EOF")?;
            Ok::<_, String>((response, stream))
        })
        .await;
        let mut row = json!({"rust":is_rust,"profile":profile,"request":request.encode_to_vec(),"baseline_pd_id":baseline});
        if let Ok(Ok((mut command, mut stream))) = result {
            row["split_command"] = json!(command.split_region.is_some());
            row["peer_command"] =
                json!(command.change_peer.is_some() || command.change_peer_v2.is_some());
            row["command_raw"] = json!(command.encode_to_vec());
            command.header.as_mut().unwrap().cluster_id = 0;
            row["command_normalized"] = json!(command.encode_to_vec());
            row["command_debug"] = json!(format!("{command:?}"));
            let mut operator = client
                .get_operator(p::GetOperatorRequest {
                    header: header.clone(),
                    region_id: 2,
                })
                .await
                .unwrap()
                .into_inner();
            row["operator_raw"] = json!(operator.encode_to_vec());
            operator.header.as_mut().unwrap().cluster_id = 0;
            row["operator_normalized"] = json!(operator.encode_to_vec());
            row["operator_debug"] = json!(format!("{operator:?}"));
            drop(sender);
            row["stream_terminal"] = json!(format!(
                "{:?}",
                tokio::time::timeout(Duration::from_secs(5), stream.message()).await
            ));
        } else {
            row["error"] = json!(format!("{result:?}"));
            drop(sender);
        }
        row["after_pd_id"] = json!(
            pd.alloc_id(pdpb::AllocIdRequest {
                header: pd_header,
                count: 1
            })
            .await
            .unwrap()
            .into_inner()
            .id
        );
        let after = backend
            .read(Read::range(
                region_key.as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        row["pd_region_unchanged"] = json!(before.entries == after.entries);
        row["logs"] = json!(sidecar.logs());
        records.push(row);
        save(&records, profile);
        sidecar.stop();
        drop(backend);
        drop(pd);
        owner.stop().unwrap();
    }
    for row in &records {
        assert!(row.get("error").is_none(), "real scheduling command: {row}");
        assert_eq!(row["pd_region_unchanged"], true);
        assert_eq!(row["split_command"], profile == "split");
        assert_eq!(row["peer_command"], profile.starts_with("replica"));
        assert_eq!(
            row["after_pd_id"].as_u64().unwrap(),
            row["baseline_pd_id"].as_u64().unwrap()
                + if profile.starts_with("replica") { 2 } else { 1 },
            "planner ID consumption must match the operation: {row}"
        );
    }
    assert_eq!(
        records[0]["command_normalized"],
        records[1]["command_normalized"]
    );
    assert_eq!(
        records[0]["operator_normalized"],
        records[1]["operator_normalized"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_live_label_split_command_and_operator_status() {
    scenario("split").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_live_replica_command_and_pd_owned_peer_id() {
    scenario("replica").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_live_replica_v2_command_and_pd_owned_peer_id() {
    scenario("replica-v2").await;
}
