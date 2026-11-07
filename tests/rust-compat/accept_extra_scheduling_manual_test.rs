// Copyright 2026 AsterSQL.
use super::{accept_extra_transport_config_test::Work, accept_modes::Deployment};
use astersql_pd_proto::{metapb, pdpb, schedulingpb as p};
use astersql_pd_storage_kv::{Consistency, MetaStore, Read};
use prost::Message;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
struct Reader(tokio::task::AbortHandle);
impl Drop for Reader {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn post(
    owner: &crate::harness::Server,
    path: &str,
    body: Value,
    certs: Option<&Path>,
) -> Result<(), String> {
    let mut command = std::process::Command::new("curl");
    if let Some(certs) = certs {
        command
            .arg("--cacert")
            .arg(certs.join("ca.pem"))
            .arg("--cert")
            .arg(certs.join("allowed.pem"))
            .arg("--key")
            .arg(certs.join("allowed.key"));
    }
    let output = command
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            &body.to_string(),
            "-w",
            "\n%{http_code}",
            &format!("{}{path}", owner.url),
        ])
        .output()
        .map_err(|e| e.to_string())?;
    let body = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && body.ends_with("\n200") {
        Ok(())
    } else {
        Err(format!("setup POST {path}: {body}"))
    }
}
async fn operator(
    client: &mut p::scheduling_client::SchedulingClient<tonic::transport::Channel>,
    header: Option<p::RequestHeader>,
    success: bool,
) -> Result<p::GetOperatorResponse, String> {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let result = client
                .get_operator(p::GetOperatorRequest {
                    header: header.clone(),
                    region_id: 2,
                })
                .await
                .map_err(|e| e.to_string())?
                .into_inner();
            if result.header.as_ref().is_some_and(|h| h.error.is_none())
                && (!success || result.status == pdpb::OperatorStatus::Success as i32)
            {
                return Ok(result);
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    })
    .await
    .map_err(|_| "operator state timed out".to_owned())?
}
async fn run(
    binary: &Path,
    go: &Path,
    kind: &str,
    config: &Path,
    embedded: bool,
    certs: Option<&Path>,
) -> Result<Value, String> {
    let flag = format!("--config={}", config.display());
    let mut pd = super::accept_modes::pd_owner(
        go,
        &[&flag],
        &[("PD_SERVICE_MODE".into(), "api".into())],
        certs,
    )
    .await;
    let mut owner = pdpb::pd_client::PdClient::new(
        super::accept_modes::channel(&pd.url, certs, Some("allowed")).await?,
    );
    let cluster = owner
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .map_err(|e| e.to_string())?
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let pd_header = Some(pdpb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    // Reserve the fixture's IDs through the real PD allocator before bootstrap.
    for expected in 1..=4 {
        let allocated = owner
            .alloc_id(pdpb::AllocIdRequest {
                header: pd_header.clone(),
                count: 1,
            })
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        if allocated.id != expected {
            return Err(format!("fixture ID reservation: {allocated:?}"));
        }
    }
    let leader = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let original = metapb::Region {
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
        address: "manual-store-one".into(),
        version: "8.5.0".into(),
        node_state: metapb::NodeState::Serving as i32,
        labels: vec![metapb::StoreLabel {
            key: "zone".into(),
            value: "deny".into(),
        }],
        ..Default::default()
    };
    let response = owner
        .bootstrap(pdpb::BootstrapRequest {
            header: pd_header.clone(),
            store: Some(store1),
            region: Some(original.clone()),
        })
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    if response.header.as_ref().is_none_or(|h| h.error.is_some()) {
        return Err(format!("bootstrap rejected: {response:?}"));
    }
    let response = owner
        .put_store(pdpb::PutStoreRequest {
            header: pd_header.clone(),
            store: Some(metapb::Store {
                id: 2,
                address: "manual-store-two".into(),
                version: "8.5.0".into(),
                node_state: metapb::NodeState::Serving as i32,
                labels: vec![metapb::StoreLabel {
                    key: "zone".into(),
                    value: "allow".into(),
                }],
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    if response.header.unwrap().error.is_some() {
        return Err("store registration rejected".into());
    }
    if kind == "scatter" {
        post(
            &pd,
            "/pd/api/v1/config/label-property",
            json!({"action":"set","type":"reject-leader","label-key":"zone","label-value":"deny"}),
            certs,
        )?;
    }
    let baseline = owner
        .alloc_id(pdpb::AllocIdRequest {
            header: pd_header.clone(),
            count: 1,
        })
        .await
        .map_err(|e| e.to_string())?
        .into_inner()
        .id;
    let backend = super::accept_modes::metadata(pd.metadata_url(), certs).await;
    let before = backend
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .map_err(|e| e.to_string())?;
    let tls_argument = certs.map(|p| super::accept_modes::tls_argument(p, "scheduling"));
    let mut extra = vec!["--log-level=debug"];
    extra.extend(tls_argument.iter().map(String::as_str));
    let mut sidecar = Deployment::start(
        binary,
        pd.metadata_url(),
        "scheduling",
        cluster,
        &extra,
        embedded,
        certs,
    )
    .await;
    let header = Some(p::RequestHeader {
        cluster_id: cluster,
        sender_id: 0,
    });
    let mut client = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Ok(channel) =
                super::accept_modes::channel(&sidecar.url, certs, Some("allowed")).await
            {
                break p::scheduling_client::SchedulingClient::new(channel);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .map_err(|_| "Scheduling listener readiness timed out".to_owned())?;
    for id in [1, 2] {
        tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                if sidecar.failed()? {
                    return Err(format!("sidecar exited: {}", sidecar.logs()));
                }
                let response = client
                    .store_heartbeat(p::StoreHeartbeatRequest {
                        header: header.clone(),
                        stats: Some(pdpb::StoreStats {
                            store_id: id,
                            capacity: 100 * 1024 * 1024 * 1024,
                            available: 90 * 1024 * 1024 * 1024,
                            region_count: 1,
                            ..Default::default()
                        }),
                    })
                    .await;
                if response.is_ok_and(|r| r.into_inner().header.is_some_and(|h| h.error.is_none()))
                {
                    return Ok::<_, String>(());
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .map_err(|_| "store watch readiness timed out".to_owned())??;
    }
    let mut region = original;
    region.start_key = b"a".to_vec();
    region.end_key = b"w".to_vec();
    region.peers.push(metapb::Peer {
        id: 4,
        store_id: 2,
        ..Default::default()
    });
    region.region_epoch = Some(metapb::RegionEpoch {
        conf_ver: 2,
        version: 2,
    });
    let report = p::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(region.clone()),
        leader: Some(leader.clone()),
        approximate_size: 1024 * 1024,
        approximate_keys: 1,
        ..Default::default()
    };
    // Explicit invalid-store barrier proves the preceding valid report was read.
    let mut barrier = report.clone();
    barrier.leader.as_mut().unwrap().store_id = 999;
    let mut probe = client.clone();
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = probe
            .region_heartbeat(tokio_stream::iter([report.clone(), barrier]))
            .await?
            .into_inner();
        loop {
            if stream.message().await?.is_none() {
                return Ok::<_, tonic::Status>(());
            }
        }
    })
    .await
    .map_err(|_| "heartbeat admission barrier timed out".to_owned())?;
    if !result.is_err_and(|e| {
        e.code() == tonic::Code::Unknown && e.message() == "invalid store ID 999, not found"
    }) {
        return Err("heartbeat admission barrier mismatch".into());
    }
    let bucket_request = p::RegionBucketsRequest {
        header: header.clone(),
        region_epoch: region.region_epoch.clone(),
        buckets: Some(metapb::Buckets {
            region_id: 2,
            version: 4,
            keys: vec![b"a".to_vec(), b"h".to_vec(), b"w".to_vec()],
            stats: Some(metapb::BucketStats {
                read_bytes: vec![100, 200],
                write_bytes: vec![0, 0],
                read_qps: vec![0, 0],
                write_qps: vec![0, 0],
                read_keys: vec![0, 0],
                write_keys: vec![0, 0],
            }),
            period_in_ms: 1000,
        }),
    };
    let mut buckets = client
        .region_buckets(tokio_stream::iter([bucket_request.clone()]))
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    let mut bucket_ack = tokio::time::timeout(Duration::from_secs(5), buckets.message())
        .await
        .map_err(|_| "bucket acknowledgement timeout")?
        .map_err(|e| e.to_string())?
        .ok_or("bucket early EOF")?;
    let bucket_ack_raw = bucket_ack.encode_to_vec();
    if bucket_ack.header.as_ref().is_none_or(|h| h.error.is_some()) {
        return Err(format!("bucket response {bucket_ack:?}"));
    }
    bucket_ack.header.as_mut().unwrap().cluster_id = 0;
    if buckets
        .message()
        .await
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("extra bucket response".into());
    }
    let (send, receive) = tokio::sync::mpsc::channel(8);
    send.send(report.clone()).await.map_err(|e| e.to_string())?;
    let (events, mut next) = tokio::sync::mpsc::channel(8);
    let mut heartbeats = client.clone();
    let reader = tokio::spawn(async move {
        let mut frames = vec![];
        let work = async {
            let mut stream = heartbeats
                .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(receive))
                .await?
                .into_inner();
            while let Some(frame) = stream.message().await? {
                frames.push(frame.encode_to_vec());
                if events.send(frame).await.is_err() {
                    break;
                }
            }
            Ok::<_, tonic::Status>(())
        }
        .await;
        json!({"frames":frames,"terminal":match work {Ok(())=>json!({"kind":"EOF"}),Err(e)=>json!({"kind":"status","code":format!("{:?}",e.code()),"message":e.message()})}})
    });
    let _reader = Reader(reader.abort_handle());
    let mut caller = client.clone();
    let case_header = header.clone();
    let action = kind.to_owned();
    let request = if kind == "split" {
        p::SplitRegionsRequest {
            header: header.clone(),
            split_keys: vec![b"m".to_vec()],
            retry_limit: 0,
        }
        .encode_to_vec()
    } else {
        p::ScatterRegionsRequest {
            header: header.clone(),
            regions_id: vec![2],
            group: "manual-completion".into(),
            retry_limit: 0,
            skip_store_limit: true,
        }
        .encode_to_vec()
    };
    let mut task = tokio::spawn(async move {
        if action == "split" {
            let mut r = caller
                .split_regions(p::SplitRegionsRequest {
                    header: case_header,
                    split_keys: vec![b"m".to_vec()],
                    retry_limit: 0,
                })
                .await
                .map_err(|e| e.to_string())?
                .into_inner();
            let raw = r.encode_to_vec();
            r.header.as_mut().unwrap().cluster_id = 0;
            Ok::<_, String>(
                json!({"raw":raw,"normalized":r.encode_to_vec(),"percentage":r.finished_percentage,"regions":r.regions_id,"error":r.header.unwrap().error.map(|e|e.message)}),
            )
        } else {
            let mut attempts = Vec::new();
            let mut r = loop {
                let r = caller
                    .scatter_regions(p::ScatterRegionsRequest {
                        header: case_header.clone(),
                        regions_id: vec![2],
                        group: "manual-completion".into(),
                        retry_limit: 0,
                        skip_store_limit: true,
                    })
                    .await
                    .map_err(|e| e.to_string())?
                    .into_inner();
                attempts.push(r.encode_to_vec());
                let current = caller
                    .get_operator(p::GetOperatorRequest {
                        header: case_header.clone(),
                        region_id: 2,
                    })
                    .await
                    .map_err(|e| e.to_string())?
                    .into_inner();
                if current.header.as_ref().is_some_and(|h| h.error.is_none()) {
                    break r;
                }
                if attempts.len() >= 8 {
                    return Err(format!(
                        "scatter produced no operator after receipts: {attempts:?}"
                    ));
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            };
            let raw = r.encode_to_vec();
            r.header.as_mut().unwrap().cluster_id = 0;
            Ok(
                json!({"raw":raw,"normalized":r.encode_to_vec(),"percentage":r.finished_percentage,"failed":r.failed_regions_id,"attempts":attempts,"error":r.header.unwrap().error.map(|e|e.message)}),
            )
        }
    });
    let mut command = match tokio::time::timeout(Duration::from_secs(15), next.recv()).await {
        Ok(Some(command)) => command,
        other => {
            let receipt = if task.is_finished() {
                format!("{:?}", (&mut task).await)
            } else {
                "still pending".into()
            };
            return Err(format!(
                "{kind} command unavailable {other:?}; RPC: {receipt}; {}",
                sidecar.logs()
            ));
        }
    };
    let command_raw = command.encode_to_vec();
    command.header.as_mut().unwrap().cluster_id = 0;
    let mut running = operator(&mut client, header.clone(), false).await?;
    let running_raw = running.encode_to_vec();
    running.header.as_mut().unwrap().cluster_id = 0;
    let mut allocations = Value::Null;
    if kind == "split" {
        let split = command.split_region.as_ref().ok_or("not a split command")?;
        if split.keys != vec![b"m".to_vec()] || split.policy != 2 {
            return Err(format!("split command: {command:?}"));
        }
        if task.is_finished() {
            return Err("split RPC completed before any observed split".into());
        }
        // A missing cached leader rejects before allocation and triggers discovery.
        let ids = tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let result = client
                    .ask_batch_split(p::AskBatchSplitRequest {
                        header: header.clone(),
                        region: Some(region.clone()),
                        split_count: 1,
                        ..Default::default()
                    })
                    .await;
                match result {
                    Ok(response) => break Ok(response.into_inner()),
                    Err(error)
                        if error.code() == tonic::Code::Unknown
                            && error.message() == "PD leader is not found" =>
                    {
                        tokio::time::sleep(Duration::from_millis(25)).await
                    }
                    Err(error) => break Err(error.to_string()),
                }
            }
        })
        .await
        .map_err(|_| "PD leader discovery timed out".to_owned())??;
        if ids.header.as_ref().is_none_or(|h| h.error.is_some())
            || ids.ids.len() != 1
            || ids.ids[0].new_peer_ids.len() != 2
        {
            return Err(format!("PD-owned split IDs: {ids:?}"));
        }
        let allocated = &ids.ids[0];
        let mut normalized = ids.clone();
        normalized.header.as_mut().unwrap().cluster_id = 0;
        allocations = json!({"raw":ids.encode_to_vec(),"normalized":normalized.encode_to_vec(),"new_id":allocated.new_region_id});
        let mut left = region.clone();
        left.end_key = b"m".to_vec();
        left.region_epoch.as_mut().unwrap().version += 1;
        let mut right = region.clone();
        right.id = allocated.new_region_id;
        right.start_key = b"m".to_vec();
        right.region_epoch.as_mut().unwrap().version += 1;
        for (peer, id) in right.peers.iter_mut().zip(&allocated.new_peer_ids) {
            peer.id = *id;
        }
        for child in [left, right] {
            let mut ack = report.clone();
            ack.leader = Some(child.peers[0].clone());
            ack.region = Some(child);
            send.send(ack).await.map_err(|e| e.to_string())?;
        }
    } else {
        let target = command
            .transfer_leader
            .as_ref()
            .and_then(|r| r.peer.as_ref())
            .ok_or("not a leader-transfer scatter command")?;
        if target.store_id != 2 || running.desc != b"scatter-region" {
            return Err(format!(
                "manual scatter command/operator: {command:?} {running:?}"
            ));
        }
        let mut ack = report.clone();
        ack.leader = Some(target.clone());
        send.send(ack).await.map_err(|e| e.to_string())?;
    }
    let response = tokio::time::timeout(Duration::from_secs(20), task)
        .await
        .map_err(|_| "manual RPC completion timed out".to_owned())?
        .map_err(|e| e.to_string())??;
    if response["percentage"] != 100 || !response["error"].is_null() {
        return Err(format!("manual RPC result: {response}"));
    }
    if kind == "split" && response["regions"] != json!([allocations["new_id"].as_u64().unwrap()]) {
        return Err(format!("observed child IDs: {response}"));
    }
    let mut success = operator(&mut client, header.clone(), true).await?;
    let success_raw = success.encode_to_vec();
    success.header.as_mut().unwrap().cluster_id = 0;
    drop(send);
    let terminal = tokio::time::timeout(Duration::from_secs(5), reader)
        .await
        .map_err(|_| "heartbeat close timed out".to_owned())?
        .map_err(|e| e.to_string())?;
    let after_id = owner
        .alloc_id(pdpb::AllocIdRequest {
            header: pd_header,
            count: 1,
        })
        .await
        .map_err(|e| e.to_string())?
        .into_inner()
        .id;
    let mut tls_refusals = vec![];
    if let Some(certs) = certs {
        for identity in [None, Some("denied"), Some("rogue")] {
            let response =
                match super::accept_modes::channel(&sidecar.url, Some(certs), identity).await {
                    Ok(channel) => p::scheduling_client::SchedulingClient::new(channel)
                        .get_operator(p::GetOperatorRequest {
                            header: header.clone(),
                            region_id: 2,
                        })
                        .await
                        .map(|r| format!("{:?}", r.into_inner()))
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(e),
                };
            if response.is_ok() {
                return Err(format!("TLS identity {identity:?} accepted"));
            }
            tls_refusals
                .push(json!({"identity":identity,"accepted":false,"raw":format!("{response:?}")}));
        }
    }
    sidecar.stop().await;
    let after = backend
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .map_err(|e| e.to_string())?;
    let encode = |entries: &[astersql_pd_storage_kv::Entry]| {
        entries.iter().map(|e| json!({"key":e.key,"value":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision,"version":e.version,"lease":e.lease})).collect::<Vec<_>>()
    };
    let row = json!({"kind":kind,"tls_refusals":tls_refusals,"bucket_request":bucket_request.encode_to_vec(),"bucket_ack_raw":bucket_ack_raw,"bucket_ack_normalized":bucket_ack.encode_to_vec(),"request":request,"response":response,"command_raw":command_raw,"command_normalized":command.encode_to_vec(),"command_debug":format!("{command:?}"),"running_raw":running_raw,"running_normalized":running.encode_to_vec(),"running_debug":format!("{running:?}"),"success_raw":success_raw,"success_normalized":success.encode_to_vec(),"allocations":allocations,"terminal":terminal,"baseline_pd_id":baseline,"after_pd_id":after_id,"namespace_unchanged":before.entries==after.entries,"namespace_before":encode(&before.entries),"namespace_after":encode(&after.entries),"log":sidecar.logs()});
    drop(backend);
    drop(owner);
    pd.stop()?;
    Ok(row)
}
pub(super) async fn scenario(kind: &str) {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let work = Work(std::env::temp_dir().join(format!("pd-manual-{kind}-{}", std::process::id())));
    std::fs::create_dir(&work.0).unwrap();
    let config = work.0.join("pd.toml");
    std::fs::write(&config,"cluster-version={Major=8,Minor=5,Patch=0}\n[schedule]\nleader-schedule-limit=0\nregion-schedule-limit=0\nhot-region-schedule-limit=0\n[replication]\nmax-replicas=2\nenable-placement-rules=true\n").unwrap();
    let mut rows = vec![];
    let certs = super::accept_extra_transport_config_test::certificates(&format!(
        "scheduling-{kind}-business"
    ));
    for (binary, is_rust, embedded, tls) in [
        (&go, false, false, false),
        (&rust, true, false, false),
        (&rust, true, true, false),
        (&go, false, false, true),
        (&rust, true, false, true),
        (&rust, true, true, true),
    ] {
        let result = tokio::time::timeout(
            Duration::from_secs(65),
            run(
                binary,
                &go,
                kind,
                &config,
                embedded,
                tls.then_some(certs.0.as_path()),
            ),
        )
        .await;
        let mut row = match result {
            Ok(Ok(row)) => row,
            other => json!({"error":format!("{other:?}")}),
        };
        row["rust"] = json!(is_rust);
        row["embedded"] = json!(embedded);
        row["tls"] = json!(tls);
        rows.push(row);
        std::fs::write(
            PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
                .with_extension(format!("manual-{kind}.json")),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
    }
    for row in &rows {
        assert!(row.get("error").is_none(), "manual domain: {row}");
        assert_eq!(row["namespace_unchanged"], true, "{row}");
        assert_eq!(
            row["after_pd_id"].as_u64().unwrap(),
            row["baseline_pd_id"].as_u64().unwrap() + if kind == "split" { 4 } else { 1 },
            "{row}"
        );
    }
    for field in [
        "bucket_ack_normalized",
        "command_normalized",
        "running_normalized",
        "success_normalized",
    ] {
        for index in [1, 2, 3, 4, 5] {
            assert_eq!(rows[0][field], rows[index][field], "{field}");
        }
    }
    for index in [1, 2, 3, 4, 5] {
        assert_eq!(
            rows[0]["response"]["normalized"],
            rows[index]["response"]["normalized"]
        );
        assert_eq!(
            rows[0]["terminal"]["terminal"],
            rows[index]["terminal"]["terminal"]
        );
        if kind == "split" {
            assert_eq!(
                rows[0]["allocations"]["normalized"],
                rows[index]["allocations"]["normalized"]
            );
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_manual_split_observes_children_before_completion() {
    scenario("split").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_manual_scatter_emits_and_completes_transfer() {
    scenario("scatter").await;
}
