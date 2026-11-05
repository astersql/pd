// Copyright 2026 AsterSQL.
use crate::meta_group_assignment_oracle_test::http;
use astersql_pd_core::RegionsInfo;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::{
    BoundType, KeyspaceRegionSplit, KeyspaceSplitConfig, make_region_bound,
};
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_domain::{Consistency, MetaStore, Read, StorageEndpoint};
use serde_json::{Value, json};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

async fn label(store: &dyn MetaStore, cluster: u64, id: u32) -> Value {
    let key = ClusterPaths::new(cluster).region_label_key_path(&format!("keyspaces/{id}"));
    let s = store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap();
    json!({"key":key,"revision":s.revision,"entries":s.entries.iter().map(|e|json!({"bytes":String::from_utf8_lossy(&e.value),"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>()})
}
fn configure(url: &str, wait: bool, timeout: &str) {
    let r = http(
        &format!("{url}/pd/api/v1/config"),
        "POST",
        json!({"keyspace.wait-region-split":wait,"keyspace.check-region-split-interval":"20ms","keyspace.wait-region-split-timeout":timeout}),
    );
    assert_eq!(r.0, 200, "{r:?}");
}
fn create(url: String, id: u32) -> (u16, Value) {
    http(
        &format!("{url}/pd/api/v2/keyspaces/id"),
        "POST",
        json!({"id":id,"name":format!("oracle36_{id}"),"config":{}}),
    )
}
async fn published(url: &str, labeler: &Mutex<RegionLabeler>, id: u32) {
    let name = format!("keyspaces/{id}");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let rules = http(
                &format!("{url}/pd/api/v1/config/region-label/rules"),
                "GET",
                Value::Null,
            );
            assert_eq!(rules.0, 200, "{rules:?}");
            if rules.1.as_array().unwrap().iter().any(|r| r["id"] == name)
                && labeler.lock().await.index().rule(&name).is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn boundaries(url: &str, cluster: u64, id: u32, regions: &mut RegionsInfo) -> Value {
    let b = make_region_bound(id);
    let (left, right) = BoundType::Txn.bounds(&b);
    let mut requests = vec![];
    for (n, start, end) in [
        (0, left.to_vec(), right.to_vec()),
        (1, right.to_vec(), vec![]),
    ] {
        let region_id = 10000 + u64::from(id) * 10 + n;
        let peer = metapb::Peer {
            id: region_id + 1,
            store_id: 1,
            ..Default::default()
        };
        requests.push(pdpb::RegionHeartbeatRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: region_id,
                start_key: start,
                end_key: end,
                region_epoch: Some(metapb::RegionEpoch {
                    conf_ver: 1,
                    version: 100 + u64::from(id),
                }),
                peers: vec![peer.clone()],
                ..Default::default()
            }),
            leader: Some(peer),
            approximate_size: 1024 * 1024,
            ..Default::default()
        });
    }
    let mut client = PdClient::connect(url.to_string()).await.unwrap();
    match client
        .region_heartbeat(tokio_stream::iter(requests.clone()))
        .await
    {
        Ok(r) => {
            let mut stream = r.into_inner();
            loop {
                match stream.message().await {
                    Ok(Some(r)) => assert!(r.header.unwrap().error.is_none()),
                    Ok(None) => break,
                    Err(e) if e.code() == tonic::Code::Unknown && e.message() == "EOF" => break,
                    Err(e) => panic!("heartbeat: {e}"),
                }
            }
        }
        Err(e) if e.code() == tonic::Code::Unknown && e.message() == "EOF" => {}
        Err(e) => panic!("heartbeat headers: {e}"),
    }
    let mut observed = vec![];
    for key in [left, right] {
        let meta = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let r = client
                    .get_region(pdpb::GetRegionRequest {
                        header: Some(pdpb::RequestHeader {
                            cluster_id: cluster,
                            ..Default::default()
                        }),
                        region_key: key.to_vec(),
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner();
                assert!(r.header.unwrap().error.is_none());
                if let Some(r) = r.region
                    && r.start_key == key
                {
                    break r;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        observed
            .push(json!({"region_id":meta.id,"start_key":meta.start_key,"end_key":meta.end_key}));
    }
    for request in requests {
        regions.put_region(Arc::new(
            astersql_pd_server::core_region_adapter::region_from_heartbeat(request, 1024 * 1024)
                .unwrap(),
        ));
    }
    json!(observed)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_036_keyspace_split_go_differential() {
    let (mut go, cluster) = crate::group_merge_oracle_test::start_bootstrapped_go(None).await;
    let fixture = crate::harness::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let go_store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(
            StorageEndpoint::new(store.clone()),
            ClusterPaths::new(42),
            0,
        )
        .await
        .unwrap(),
    ));
    let mut regions = RegionsInfo::new();
    let config = Arc::new(RwLock::new(KeyspaceSplitConfig {
        interval: Duration::from_millis(20),
        timeout: Duration::from_millis(200),
    }));
    let split =
        KeyspaceRegionSplit::new(Some(labeler.clone()), Some(regions.clone()), config.clone());
    let mut receipts = vec![];
    configure(&go.url, true, "200ms");
    let response = create(go.url.clone(), 51);
    let error = split
        .split(&CancellationToken::new(), 51, true, BoundType::Txn, 0)
        .await
        .unwrap_err();
    assert_eq!(response, (500, json!(error.to_string())));
    let g = label(&go_store, cluster, 51).await;
    let r = label(store.as_ref(), 42, 51).await;
    assert_eq!(g["entries"], json!([]));
    assert_eq!(r["entries"], json!([]));
    receipts.push(
        json!({"case":"timeout","http":response,"rust_error":error.to_string(),"go":g,"rust":r}),
    );
    configure(&go.url, true, "3s");
    config.write().unwrap().timeout = Duration::from_secs(3);
    for id in [52, 53, 54] {
        let mut observed = Value::Null;
        if id == 53 {
            observed = boundaries(&go.url, cluster, id, &mut regions).await;
        }
        if id == 54 {
            configure(&go.url, false, "3s");
        }
        let url = go.url.clone();
        let go_task = tokio::task::spawn_blocking(move || create(url, id));
        let worker = split.clone();
        let rust_task = tokio::spawn(async move {
            worker
                .split(&CancellationToken::new(), id, id != 54, BoundType::Txn, 0)
                .await
        });
        if id == 52 {
            published(&go.url, &labeler, id).await;
            observed = boundaries(&go.url, cluster, id, &mut regions).await;
        }
        let response = go_task.await.unwrap();
        assert_eq!(response.0, 200, "{response:?}");
        rust_task.await.unwrap().unwrap();
        let g = label(&go_store, cluster, id).await;
        let r = label(store.as_ref(), 42, id).await;
        assert_eq!(
            serde_json::from_str::<Value>(g["entries"][0]["bytes"].as_str().unwrap()).unwrap(),
            serde_json::from_str::<Value>(r["entries"][0]["bytes"].as_str().unwrap()).unwrap(),
        );
        assert!(!g["entries"].as_array().unwrap().is_empty());
        receipts.push(json!({"case":match id {52=>"arrival",53=>"preexisting",_=>"no_wait"},"http":response,"regions":observed,"go":g,"rust":r}));
    }
    configure(&go.url, true, "30s");
    config.write().unwrap().timeout = Duration::from_secs(30);
    let url = format!("{}/pd/api/v2/keyspaces/id", go.url);
    // Shutdown can terminate the HTTP transport. Preserve its actual outcome rather than inventing a status.
    let go_task = tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("curl")
            .args([
                "--silent",
                "--show-error",
                "--max-time",
                "40",
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "-d",
                &json!({"id":55,"name":"oracle36_55","config":{}}).to_string(),
                "-w",
                "\n%{http_code}",
                &url,
            ])
            .output()
            .unwrap();
        json!({"exit_code":output.status.code(),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)})
    });
    let cancel = CancellationToken::new();
    let worker = split.clone();
    let request_cancel = cancel.clone();
    let rust_task = tokio::spawn(async move {
        worker
            .split(&request_cancel, 55, true, BoundType::Txn, 0)
            .await
    });
    published(&go.url, &labeler, 55).await;
    go.stop().unwrap();
    let logs = go.logs();
    assert!(
        logs.lines()
            .any(|line| line.contains("wait region split meets error")
                && line.contains("keyspace-id=55")
                && line.contains("wait region split canceled")),
        "{logs}"
    );
    let go_response = go_task.await.unwrap();
    assert_eq!(go_response["exit_code"], 0);
    assert_eq!(
        go_response["stdout"],
        "\"[keyspace] wait region split canceled\"\n500"
    );
    // The Go process also closes the labeler's persistence lifetime on shutdown.
    // Stop the actual Rust storage before cancelling its independent waiting owner.
    let mut fixture = fixture;
    fixture.command("stop", "stopped");
    cancel.cancel();
    let error = rust_task.await.unwrap().unwrap_err();
    assert_eq!(error.to_string(), "[keyspace] wait region split canceled");
    assert!(error.cleanup_error.is_some());
    fixture.command("start", "restarted");
    let restored_store = fixture.store().await;
    let binary = crate::readonly_oracle::pinned_go_binary().unwrap();
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let restored_go =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let g = label(&restored_go, cluster, 55).await;
    let r = label(&restored_store, 42, 55).await;
    assert_eq!(g["entries"].as_array().unwrap().len(), 1);
    assert_eq!(r["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(g["entries"][0]["bytes"].as_str().unwrap()).unwrap(),
        serde_json::from_str::<Value>(r["entries"][0]["bytes"].as_str().unwrap()).unwrap()
    );
    receipts.push(json!({"case":"shutdown_cancellation_same_wal","http_transport":go_response,"rust_error":error.to_string(),"rust_cleanup_error":error.cleanup_error.map(|e|e.to_string()),"go_log":logs.lines().filter(|line|line.contains("keyspace-id=55")).collect::<Vec<_>>(),"go":g,"rust":r}));
    if let Some(path) = std::env::var_os("MCS_036_ORACLE_REPORT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({"cluster_id":cluster,"cases":receipts})).unwrap(),
        )
        .unwrap();
    }
}
