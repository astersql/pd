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
    let native = profile.starts_with("native-");
    let case = profile.strip_prefix("native-").unwrap_or(profile);
    let live_config = case.contains("watch");
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
            if case.starts_with("replica-v2") {
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
        let environment = [("PD_SERVICE_MODE".into(), "api".into())];
        let mut owner = if native && is_rust {
            crate::harness::Server::start_rust_with_environment(&rust, &[&arg], &environment)
        } else {
            crate::harness::Server::start_with_environment(&go, &[&arg], &environment)
        }
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
            version: if case.starts_with("replica-v2") {
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
                    version: if case.starts_with("replica-v2") {
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
        let backend =
            EtcdMetaStore::connect(&[owner.metadata_url().into()], Duration::from_secs(2))
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
        let mut config_updates = vec![];
        let mut updated_config = None;
        if live_config {
            config_updates.push(set_replicas(&owner, 1).unwrap());
        }
        let baseline = pd
            .alloc_id(pdpb::AllocIdRequest {
                header: pd_header.clone(),
                count: 1,
            })
            .await
            .unwrap()
            .into_inner()
            .id;
        let discovery = if native && is_rust {
            vec!["--pd-discovery=primary-key"]
        } else {
            vec![]
        };
        let mut sidecar = Sidecar::start_service_with_args(
            binary,
            owner.metadata_url(),
            "scheduling",
            "http",
            &discovery,
        );
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
        if case.starts_with("replica") {
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
        let result = {
            let first_response = async {
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
            };
            tokio::pin!(first_response);
            if live_config {
                match tokio::time::timeout(Duration::from_millis(700), &mut first_response).await {
                    Ok(early) => Ok(Err(format!("command before live config update: {early:?}"))),
                    Err(_) => {
                        config_updates.push(set_replicas(&owner, 2).unwrap());
                        let key = astersql_pd_keypath::ClusterPaths::new(cluster).config_path();
                        updated_config = Some(
                            backend
                                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                                .await
                                .unwrap()
                                .entries,
                        );
                        tokio::time::timeout(Duration::from_secs(30), &mut first_response).await
                    }
                }
            } else {
                tokio::time::timeout(Duration::from_secs(30), &mut first_response).await
            }
        };
        let consensus_client = backend.consensus_leader_client_url().await.unwrap();
        let primary_key = astersql_pd_keypath::ClusterPaths::new(cluster).election_path(None);
        let primary = backend
            .read(Read::range(
                primary_key.as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        let primary_member = pdpb::Member::decode(primary.entries[0].value.as_slice()).unwrap();
        assert!(primary_member.client_urls.contains(&owner.url));
        if native && is_rust {
            assert_ne!(consensus_client.as_deref(), Some(owner.url.as_str()));
        }
        let mut row = json!({"metadata_url":owner.metadata_url(),"pd_url":owner.url,"consensus_client":consensus_client,"primary_member_raw":primary.entries[0].value,"config_updates":config_updates,"rust":is_rust,"profile":profile,"request":request.encode_to_vec(),"baseline_pd_id":baseline});
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
            if case.ends_with("complete") {
                row["completion"] = match tokio::time::timeout(
                    Duration::from_secs(20),
                    complete_replica(&mut client, &sender, &mut stream, &request, &command),
                )
                .await
                {
                    Ok(Ok(value)) => value,
                    other => json!({"error":format!("{other:?}")}),
                };
            }
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
        if let Some(expected) = updated_config {
            let key = astersql_pd_keypath::ClusterPaths::new(cluster).config_path();
            let actual = backend
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            row["config_pd_owned"] = json!(expected == actual.entries);
            row["pd_updated_config"] = json!(
                expected
                    .iter()
                    .map(|e| json!({"value":e.value,"mod_revision":e.mod_revision}))
                    .collect::<Vec<_>>()
            );
        }
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
        if live_config {
            assert_eq!(
                row["config_pd_owned"], true,
                "MCS must not write observed PD config: {row}"
            );
        }
        assert_eq!(row["split_command"], case == "split");
        assert_eq!(row["peer_command"], case.starts_with("replica"));
        assert_eq!(
            row["after_pd_id"].as_u64().unwrap(),
            row["baseline_pd_id"].as_u64().unwrap()
                + if case.starts_with("replica") { 2 } else { 1 },
            "planner ID consumption must match the operation: {row}"
        );
    }
    if case.ends_with("complete") {
        for row in &records {
            assert!(
                row["completion"].get("error").is_none(),
                "replica completion: {row}"
            );
        }
        assert_eq!(
            records[0]["completion"]["promotion_normalized"],
            records[1]["completion"]["promotion_normalized"]
        );
        assert_eq!(
            records[0]["completion"]["success_normalized"],
            records[1]["completion"]["success_normalized"]
        );
        assert_eq!(
            records[0]["completion"]["acknowledgements"],
            records[1]["completion"]["acknowledgements"]
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

// Apply only the exact peer changes received on the real heartbeat stream.
async fn complete_replica(
    client: &mut p::scheduling_client::SchedulingClient<tonic::transport::Channel>,
    sender: &tokio::sync::mpsc::Sender<p::RegionHeartbeatRequest>,
    stream: &mut tonic::Streaming<p::RegionHeartbeatResponse>,
    initial: &p::RegionHeartbeatRequest,
    command: &p::RegionHeartbeatResponse,
) -> Result<Value, String> {
    let change = command
        .change_peer
        .clone()
        .or_else(|| command.change_peer_v2.as_ref()?.changes.first().cloned())
        .ok_or("missing learner change")?;
    if change.change_type != astersql_pd_proto::eraftpb::ConfChangeType::AddLearnerNode as i32 {
        return Err(format!("expected learner: {change:?}"));
    }
    let peer = change.peer.ok_or("missing learner peer")?;
    let mut request = initial.clone();
    let region = request.region.as_mut().unwrap();
    region.peers.push(peer.clone());
    region.region_epoch.as_mut().unwrap().conf_ver += 1;
    let first = request.encode_to_vec();
    let mut first_request = request.clone();
    sender
        .send(request.clone())
        .await
        .map_err(|e| e.to_string())?;
    let mut observed = vec![];
    let mut promotion = loop {
        let response = stream
            .message()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("EOF before promotion")?;
        observed.push(response.encode_to_vec());
        let change = response
            .change_peer
            .as_ref()
            .or_else(|| response.change_peer_v2.as_ref()?.changes.first());
        if change.is_some_and(|c| {
            c.change_type == astersql_pd_proto::eraftpb::ConfChangeType::AddNode as i32
                && c.peer
                    .as_ref()
                    .is_some_and(|p| p.id == peer.id && p.store_id == peer.store_id)
        }) {
            break response;
        }
    };
    let promotion_raw = promotion.encode_to_vec();
    promotion.header.as_mut().unwrap().cluster_id = 0;
    let region = request.region.as_mut().unwrap();
    region
        .peers
        .iter_mut()
        .find(|p| p.id == peer.id)
        .unwrap()
        .role = metapb::PeerRole::Voter as i32;
    region.region_epoch.as_mut().unwrap().conf_ver += 1;
    let second = request.encode_to_vec();
    sender
        .send(request.clone())
        .await
        .map_err(|e| e.to_string())?;
    let mut success = loop {
        let response = client
            .get_operator(p::GetOperatorRequest {
                header: initial.header.clone(),
                region_id: 2,
            })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        if response.header.as_ref().is_some_and(|h| h.error.is_none())
            && response.status == pdpb::OperatorStatus::Success as i32
        {
            break response;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    };
    let success_raw = success.encode_to_vec();
    success.header.as_mut().unwrap().cluster_id = 0;
    // Independent cluster IDs are the only normalized fields in input frames.
    request.header.as_mut().unwrap().cluster_id = 0;
    first_request.header.as_mut().unwrap().cluster_id = 0;
    Ok(
        json!({"promotion_raw":promotion_raw,"promotion_normalized":promotion.encode_to_vec(),"observed_commands":observed,"success_raw":success_raw,"success_normalized":success.encode_to_vec(),"success_debug":format!("{success:?}"),"acknowledgements_raw":[first,second],"acknowledgements":[first_request.encode_to_vec(),request.encode_to_vec()]}),
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_ack_replica_promotes_and_completes() {
    scenario("replica-complete").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_ack_replica_v2_promotes_and_completes() {
    scenario("replica-v2-complete").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_native_pd_replica_ack_uses_real_pd_discovery() {
    scenario("native-replica-complete").await;
}

fn set_replicas(owner: &crate::harness::Server, count: u64) -> Result<Value, String> {
    let body = json!({"max-replicas":count,"enable-placement-rules":"false"}).to_string();
    let result = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            &body,
            "-w",
            "\n%{http_code}",
            &format!("{}/pd/api/v1/config/replicate", owner.url),
        ])
        .output()
        .map_err(|e| e.to_string())?;
    let response = String::from_utf8_lossy(&result.stdout).into_owned();
    if !result.status.success() || !response.ends_with("\n200") {
        return Err(format!(
            "replication config update: {response}, {:?}",
            result.stderr
        ));
    }
    Ok(json!({"body":body,"response":response}))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_live_replication_config_watch_drives_replica_completion() {
    scenario("replica-watch-complete").await;
}
