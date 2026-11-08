// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;
fn scrape(url: &str) -> String {
    let out = std::process::Command::new("curl")
        .args(["-fsS", "--max-time", "5", &format!("{url}/metrics")])
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}
fn sample(text: &str, key: &str) -> f64 {
    text.lines()
        .find_map(|line| {
            line.strip_prefix(&format!("{key} "))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0.0)
}
async fn scenario(url: &str) -> BTreeMap<String, f64> {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let before = scrape(url);
    let min = client
        .get_min_ts(pdpb::GetMinTsRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(min.timestamp.unwrap().physical > 0);
    for count in [2, 0] {
        let result = client
            .tso(tokio_stream::iter([pdpb::TsoRequest {
                header: header.clone(),
                count,
                ..Default::default()
            }]))
            .await;
        if count == 0 {
            let error = match result {
                Err(error) => error,
                Ok(response) => response.into_inner().message().await.unwrap_err(),
            };
            assert_eq!(error.code(), tonic::Code::Unknown);
        } else {
            assert_eq!(
                result
                    .unwrap()
                    .into_inner()
                    .message()
                    .await
                    .unwrap()
                    .unwrap()
                    .count,
                2
            );
        }
    }
    for _ in 0..2 {
        let r = client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: header.clone(),
                region_id: 999,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            r.header.unwrap().error.unwrap().r#type,
            pdpb::ErrorType::NotBootstrapped as i32
        );
    }
    client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:20160".into(),
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
        .unwrap();
    let heartbeat = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: header.clone(),
            stats: Some(pdpb::StoreStats {
                store_id: 1,
                capacity: 1000,
                available: 900,
                used_size: 100,
                interval: Some(pdpb::TimeInterval {
                    start_timestamp: 1,
                    end_timestamp: 2,
                }),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(heartbeat.header.unwrap().error.is_none());
    let region = metapb::Region {
        id: 2,
        region_epoch: Some(metapb::RegionEpoch {
            version: 1,
            conf_ver: 1,
        }),
        peers: vec![metapb::Peer {
            id: 3,
            store_id: 1,
            ..Default::default()
        }],
        ..Default::default()
    };
    let report = pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(region.clone()),
        leader: Some(region.peers[0].clone()),
        interval: Some(pdpb::TimeInterval {
            start_timestamp: 1,
            end_timestamp: 2,
        }),
        ..Default::default()
    };
    let mut invalid = report.clone();
    invalid.region.as_mut().unwrap().id = 0;
    let (tx, rx) = tokio::sync::mpsc::channel(2);
    tx.send(report).await.unwrap();
    tx.send(invalid).await.unwrap();
    let mut heartbeat_client = client.clone();
    let heartbeat = tokio::spawn(async move {
        heartbeat_client
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let text = scrape(url);
        if sample(
            &text,
            "pd_scheduler_region_heartbeat{address=\"127.0.0.1:20160\",status=\"invalid-region\",store=\"1\",type=\"report\"}",
        ) == 1.0
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "heartbeat was not processed"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    heartbeat.abort();
    let _ = heartbeat.await;
    drop(tx);
    let bucket = metapb::Buckets {
        region_id: 2,
        version: 1,
        keys: vec![vec![], b"m".to_vec(), vec![]],
        period_in_ms: 1500,
        stats: Some(metapb::BucketStats {
            read_bytes: vec![2, 4],
            read_keys: vec![1, 1],
            read_qps: vec![0, 0],
            write_bytes: vec![2, 4],
            write_keys: vec![1, 1],
            write_qps: vec![0, 0],
        }),
    };
    assert_eq!(
        client
            .report_buckets(tokio_stream::iter([pdpb::ReportBucketsRequest {
                header: header.clone(),
                buckets: Some(bucket),
                ..Default::default()
            }]))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unknown
    );
    let mut stream = client
        .query_region(tokio_stream::iter([pdpb::QueryRegionRequest {
            header: header.clone(),
            keys: vec![b"abc".to_vec()],
            ..Default::default()
        }]))
        .await
        .unwrap()
        .into_inner();
    let r = stream.message().await.unwrap().unwrap();
    assert!(r.header.unwrap().error.is_none());
    let after = scrape(url);
    if let Ok(directory) = std::env::var("PD_METRICS_SNAPSHOTS") {
        let kind = if std::env::var("PD_METRICS_GO_URL").as_deref() == Ok(url) {
            "go"
        } else {
            "rust"
        };
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{kind}.txt")),
            &after,
        )
        .unwrap();
    }

    let mut observed = BTreeMap::new();
    for (name, expected) in [
        ("pd_server_handle_tso_duration_seconds_count", 3.0),
        ("pd_server_handle_tso_batch_size_count", 2.0),
        ("pd_server_handle_tso_batch_size_sum", 2.0),
        ("pd_server_query_region_duration_seconds_count", 1.0),
        (
            "pd_server_region_request_cnt{caller_component=\"unknown\",caller_id=\"unknown\",event=\"failed\",request=\"GetRegionByID\"}",
            2.0,
        ),
    ] {
        let delta = sample(&after, name) - sample(&before, name);
        assert_eq!(
            delta, expected,
            "missing or incorrect real RPC measurement {name}"
        );
        observed.insert(name.to_string(), delta);
    }

    for (name, help) in [
        (
            "pd_server_handle_tso_duration_seconds",
            "Bucketed histogram of processing time (s) of handled tso requests.",
        ),
        (
            "pd_server_handle_tso_batch_size",
            "Bucketed histogram of the batch size of handled tso requests.",
        ),
        (
            "pd_server_query_region_duration_seconds",
            "Bucketed histogram of processing time (s) of region query requests.",
        ),
    ] {
        assert!(after.contains(&format!("# HELP {name} {help}\n")));
        assert!(after.contains(&format!("# TYPE {name} histogram\n")));
    }
    for (key, expected) in [
        (
            "pd_scheduler_handle_store_heartbeat_duration_seconds_count{address=\"127.0.0.1:20160\",store=\"1\"}",
            1.0,
        ),
        (
            "pd_scheduler_region_heartbeat_latency_seconds_count{address=\"127.0.0.1:20160\",store=\"1\"}",
            2.0,
        ),
        (
            "pd_scheduler_handle_region_heartbeat_duration_seconds_count{address=\"127.0.0.1:20160\",store=\"1\"}",
            1.0,
        ),
        (
            "pd_scheduler_region_heartbeat{address=\"127.0.0.1:20160\",status=\"recv\",store=\"1\",type=\"report\"}",
            2.0,
        ),
        (
            "pd_scheduler_region_heartbeat{address=\"127.0.0.1:20160\",status=\"ok\",store=\"1\",type=\"report\"}",
            1.0,
        ),
        (
            "pd_scheduler_region_heartbeat{address=\"127.0.0.1:20160\",status=\"invalid-region\",store=\"1\",type=\"report\"}",
            1.0,
        ),
        (
            "pd_scheduler_region_heartbeat{address=\"127.0.0.1:20160\",status=\"bind\",store=\"1\",type=\"report\"}",
            1.0,
        ),
        (
            "pd_server_bucket_report{address=\"127.0.0.1:20160\",status=\"recv\",store=\"1\",type=\"report\"}",
            1.0,
        ),
        (
            "pd_server_bucket_report{address=\"127.0.0.1:20160\",status=\"ok\",store=\"1\",type=\"report\"}",
            1.0,
        ),
        (
            "pd_server_bucket_report_interval_seconds_sum{address=\"127.0.0.1:20160\",store=\"1\"}",
            1.0,
        ),
        (
            "pd_server_handle_bucket_report_duration_seconds_count{address=\"127.0.0.1:20160\",store=\"1\"}",
            1.0,
        ),
    ] {
        let delta = sample(&after, key) - sample(&before, key);
        assert_eq!(delta, expected, "heartbeat measurement {key}");
        observed.insert(key.into(), delta);
    }
    observed.extend(operator_disappearance(url, &mut client, header.clone()).await);
    if let Ok(directory) = std::env::var("PD_METRICS_SNAPSHOTS") {
        let kind = if std::env::var("PD_METRICS_GO_URL").as_deref() == Ok(url) {
            "go"
        } else {
            "rust"
        };
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{kind}.txt")),
            scrape(url),
        )
        .unwrap();
    }
    println!(
        "task086 rpc_metrics {}",
        serde_json::to_string(&observed).unwrap()
    );
    observed
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_086_rpc_measurements() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 8690,
                member_id: 86,
                name: "rpc-metrics".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let rust = scenario(&url).await;
    if let Ok(go) = std::env::var("PD_METRICS_GO_URL") {
        assert_eq!(rust, scenario(&go).await);
    }
    node.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_086_pd_forward_is_not_mcs_failure() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut nodes = Vec::new();
    let mut urls = Vec::new();
    let mut registries = Vec::new();
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let registry = Arc::new(prometheus::Registry::new());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 8695,
                        member_id: id,
                        name: format!("forward{id}"),
                        client_url: url.clone(),
                        peer_url: format!("http://127.0.0.1:{}", 2380 + id),
                    },
                    listener,
                    registry.clone(),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        urls.push(url);
        registries.push(registry);
    }
    let mut client = PdClient::connect(urls[1].clone()).await.unwrap();
    let make = |cluster, host: &str| {
        let mut r = tonic::Request::new(pdpb::StoreHeartbeatRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            stats: Some(pdpb::StoreStats {
                store_id: 1,
                ..Default::default()
            }),
            ..Default::default()
        });
        r.metadata_mut()
            .insert("pd-forwarded-host", host.parse().unwrap());
        r
    };
    assert_eq!(
        client
            .store_heartbeat(make(1, &urls[0]))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    let key = "pd_server_forward_fail_total{request=\"store_heartbeat\",type=\"send\"}";
    assert_eq!(
        sample(&scrape(&urls[1]), key),
        0.0,
        "Go's counter belongs to independent scheduling forwarding, not PD forwarding"
    );
    assert_eq!(
        client
            .store_heartbeat(make(8695, "http://untrusted.invalid:1234"))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    assert_eq!(sample(&scrape(&urls[1]), key), 0.0);
    let response = client
        .store_heartbeat(make(8695, &urls[0]))
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_some());
    assert_eq!(sample(&scrape(&urls[1]), key), 0.0);
    for node in nodes.iter_mut().rev() {
        node.shutdown().await.unwrap();
    }
    for registry in registries {
        assert!(registry.gather().is_empty());
    }
}

// Exercise disappearance deliberately: incidental scheduler activity in unrelated
// HTTP cases is not a reproducible trigger for this finite metric label.
async fn operator_disappearance(
    url: &str,
    client: &mut PdClient<tonic::transport::Channel>,
    header: Option<pdpb::RequestHeader>,
) -> [(String, f64); 2] {
    let reply = client
        .put_store(pdpb::PutStoreRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 2,
                address: "127.0.0.1:20161".into(),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(reply.header.unwrap().error.is_none());
    let reply = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: header.clone(),
            stats: Some(pdpb::StoreStats {
                store_id: 2,
                capacity: 1 << 30,
                available: 1 << 29,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(reply.header.unwrap().error.is_none());
    let http = |method: &str, path: &str, body: &str| {
        let output = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-X",
                method,
                "-H",
                "Content-Type: application/json",
                "--data-binary",
                body,
                "-w",
                "\n%{http_code}",
                &format!("{url}/pd/api/v1/{path}"),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        let (body, status) = text.rsplit_once('\n').unwrap();
        assert_eq!(status, "200", "{method} {path}: {body}");
    };
    http("DELETE", "operators", "");
    http(
        "POST",
        "operators",
        r#"{"name":"add-peer","region_id":2,"store_id":2}"#,
    );
    let operator = client
        .get_operator(pdpb::GetOperatorRequest {
            header,
            region_id: 2,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(operator.header.unwrap().error.is_none());
    let desc = String::from_utf8(operator.desc).unwrap();
    let [not_found, disappear] = ["not-found", "disappear"]
        .map(|event| format!("pd_schedule_operators_count{{event=\"{event}\",type=\"{desc}\"}}"));
    let before = scrape(url);
    http("DELETE", "admin/cache/region/2", "");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let after = scrape(url);
        let disappear_delta = sample(&after, &disappear) - sample(&before, &disappear);
        if disappear_delta > 0.0 {
            let not_found_delta = sample(&after, &not_found) - sample(&before, &not_found);
            assert_eq!(disappear_delta, 1.0, "{disappear}");
            assert_eq!(not_found_delta, 1.0, "{not_found}");
            return [(not_found, not_found_delta), (disappear, disappear_delta)];
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "missing real disappearance observation: {disappear}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
