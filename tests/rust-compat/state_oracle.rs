// Copyright 2026 AsterSQL.
//! Capture an isolated, existing Go PD process; never writes Go sources.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use prost::Message;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::{sleep, timeout};
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn http(url: &str, path: &str) -> Value {
    let out = std::process::Command::new("curl")
        .args([
            "-fsS",
            "--max-time",
            "5",
            &format!("{url}/pd/api/v1/{path}"),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{path}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn region(
    id: u64,
    start: &[u8],
    end: &[u8],
    v: u64,
    c: u64,
    term: u64,
) -> pdpb::RegionHeartbeatRequest {
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    pdpb::RegionHeartbeatRequest {
        region: Some(metapb::Region {
            id,
            start_key: start.into(),
            end_key: end.into(),
            region_epoch: Some(metapb::RegionEpoch {
                version: v,
                conf_ver: c,
            }),
            peers: vec![peer.clone()],
            ..Default::default()
        }),
        leader: Some(peer),
        term,
        bytes_written: 60_000_000,
        keys_written: 60_000,
        bytes_read: 120_000_000,
        keys_read: 120_000,
        approximate_size: 10 * 1024 * 1024,
        approximate_keys: 100,
        interval: Some(pdpb::TimeInterval {
            start_timestamp: 0,
            end_timestamp: 60,
        }),
        ..Default::default()
    }
}
#[tokio::main]
async fn main() {
    let url = std::env::var("PD_ORACLE_URL").unwrap();
    let mut c = PdClient::connect(url.clone()).await.unwrap();
    let id = c
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
    if let Ok(phase) = std::env::var("PD_STATE_ORACLE_PHASE") {
        restart_phase(&mut c, &url, header.clone(), id, &phase).await;
        return;
    }
    let response = c
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29999".into(),
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(region(1, b"", b"", 1, 1, 0).region.unwrap()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    let mut events = vec![];
    for (name, r) in [
        ("initial", region(1, b"", b"", 2, 2, 7)),
        ("duplicate", region(1, b"", b"", 2, 2, 7)),
        ("window", region(1, b"", b"", 2, 2, 7)),
        ("stale-version", region(1, b"", b"", 1, 2, 7)),
        ("stale-confver", region(1, b"", b"", 2, 1, 7)),
        ("stale-term", region(1, b"", b"", 2, 2, 6)),
        ("term-only", region(1, b"", b"", 2, 2, 8)),
        ("split-right", region(2, b"m", b"", 3, 2, 8)),
        ("split-left", region(1, b"", b"m", 3, 2, 8)),
        ("merge", region(1, b"", b"", 4, 3, 9)),
    ] {
        events.push(json!({"name":name,"kind":"region","wire":hex(&r.encode_to_vec())}));
    }
    let bucket = |version| metapb::Buckets {
        region_id: 1,
        version,
        keys: vec![vec![], b"m".to_vec(), vec![]],
        period_in_ms: 2000,
        stats: Some(metapb::BucketStats {
            read_bytes: vec![200000, 400000],
            read_keys: vec![4, 8],
            read_qps: vec![6, 12],
            write_bytes: vec![800000, 1600000],
            write_keys: vec![10, 20],
            write_qps: vec![12, 24],
        }),
        ..Default::default()
    };
    events.insert(
        3,
        json!({"name":"bucket-v3","kind":"bucket","wire":hex(&bucket(3).encode_to_vec())}),
    );
    events.insert(
        4,
        json!({"name":"bucket-old","kind":"bucket","wire":hex(&bucket(2).encode_to_vec())}),
    );
    events.insert(
        5,
        json!({"name":"bucket-duplicate","kind":"bucket","wire":hex(&bucket(3).encode_to_vec())}),
    );
    let stats = pdpb::StoreStats {
        store_id: 1,
        capacity: 1_000_000_000,
        available: 500_000_000,
        used_size: 500_000_000,
        bytes_read: 10_000_000,
        keys_read: 10000,
        bytes_written: 10_000_000,
        keys_written: 10000,
        interval: Some(pdpb::TimeInterval {
            start_timestamp: 0,
            end_timestamp: 10,
        }),
        peer_stats: vec![pdpb::PeerStat {
            region_id: 1,
            read_bytes: 10_000_000,
            read_keys: 10000,
            ..Default::default()
        }],
        ..Default::default()
    };
    for i in 0..3 {
        events.insert(6+i,json!({"name":format!("store-window-{i}"),"kind":"store","wire":hex(&stats.encode_to_vec())}));
    }
    let store = EtcdMetaStore::connect(std::slice::from_ref(&url), Duration::from_secs(3))
        .await
        .unwrap();
    let mut snapshots = vec![];
    let mut live: Option<tonic::Streaming<pdpb::SyncRegionResponse>> = None;
    let mut live_tx = None;
    let mut resume = 0;
    for (i, event) in events.iter().enumerate() {
        let raw = (0..event["wire"].as_str().unwrap().len())
            .step_by(2)
            .map(|n| u8::from_str_radix(&event["wire"].as_str().unwrap()[n..n + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let error = match event["kind"].as_str().unwrap() {
            "region" => {
                let mut request = pdpb::RegionHeartbeatRequest::decode(raw.as_slice()).unwrap();
                request.header = header.clone();
                let (tx, rx) = tokio::sync::mpsc::channel(2);
                tx.send(request).await.unwrap();
                let result = timeout(Duration::from_millis(200), async {
                    c.region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
                        .await
                        .unwrap()
                        .into_inner()
                        .message()
                        .await
                        .unwrap()
                })
                .await;
                drop(tx);
                result
                    .ok()
                    .flatten()
                    .and_then(|r| r.header.and_then(|h| h.error))
                    .map(|e| json!({"type":e.r#type,"message":e.message}))
            }
            "bucket" => {
                let (tx, rx) = tokio::sync::mpsc::channel(2);
                tx.send(pdpb::ReportBucketsRequest {
                    header: header.clone(),
                    buckets: Some(metapb::Buckets::decode(raw.as_slice()).unwrap()),
                    ..Default::default()
                })
                .await
                .unwrap();
                let result = timeout(
                    Duration::from_millis(200),
                    c.report_buckets(tokio_stream::wrappers::ReceiverStream::new(rx)),
                )
                .await;
                assert!(result.is_err());
                drop(tx);
                None
            }
            "store" => c
                .store_heartbeat(pdpb::StoreHeartbeatRequest {
                    header: header.clone(),
                    stats: Some(pdpb::StoreStats::decode(raw.as_slice()).unwrap()),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .error
                .map(|e| json!({"type":e.r#type,"message":e.message})),
            _ => unreachable!(),
        };
        sleep(Duration::from_millis(100)).await;
        let mut regions = http(&url, "regions");
        regions["regions"]
            .as_array_mut()
            .unwrap()
            .sort_by_key(|r| r["id"].as_u64().unwrap());
        let store_response = c
            .get_store(pdpb::GetStoreRequest {
                header: header.clone(),
                store_id: 1,
            })
            .await
            .unwrap()
            .into_inner();
        let raw_store = store_response.stats.unwrap_or_default();
        let mut store_meta = store_response.store.unwrap();
        store_meta.last_heartbeat = 0; // Explicit host-clock origin only.
        // LastHeartbeat is in StoreMeta, not raw report. No counters or report timestamps are removed.
        let mut hot_read = http(&url, "hotspot/regions/read");
        let hot_write = http(&url, "hotspot/regions/write");
        for kind in ["as_peer", "as_leader"] {
            if let Some(stores) = hot_read[kind].as_object_mut() {
                for s in stores.values_mut() {
                    if let Some(peers) = s["statistics"].as_array_mut() {
                        for peer in peers {
                            peer["last_update_time"] = json!("<store-heartbeat-time>");
                        }
                    }
                }
            }
        }
        let buckets = http(&url, "hotspot/buckets");
        let req = pdpb::SyncRegionRequest {
            header: header.clone(),
            member: Some(pdpb::Member {
                name: format!("theme53-{i}"),
                client_urls: vec!["http://127.0.0.1:29998".into()],
                ..Default::default()
            }),
            start_index: 0,
        };
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tx.send(req).await.unwrap();
        let mut stream = c
            .sync_regions(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner();
        let mut frames = vec![];
        loop {
            let mut f = timeout(Duration::from_secs(5), stream.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let done = f.regions.is_empty();
            f.header = None;
            frames.push(hex(&f.encode_to_vec()));
            if done {
                break;
            }
        }
        drop(stream);
        drop(tx);
        let kv = store
            .read(Read::prefix(
                format!("/pd/{id}/raft/r/").as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        let mut values = kv
            .entries
            .iter()
            .map(|e| {
                let m = metapb::Region::decode(e.value.as_slice()).unwrap();
                (m.id, hex(&m.encode_to_vec()))
            })
            .collect::<Vec<_>>();
        values.sort_by_key(|(id, _)| *id);
        let mut live_frames = vec![];
        if let Some(stream) = &mut live {
            while let Ok(Ok(Some(mut frame))) =
                timeout(Duration::from_millis(80), stream.message()).await
            {
                resume = frame.start_index + frame.regions.len() as u64;
                frame.header = None;
                live_frames.push(hex(&frame.encode_to_vec()));
            }
        }
        if i == 0 {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            tx.send(pdpb::SyncRegionRequest {
                header: header.clone(),
                member: Some(pdpb::Member {
                    name: "theme53-live".into(),
                    client_urls: vec!["http://127.0.0.1:29998".into()],
                    ..Default::default()
                }),
                start_index: 0,
            })
            .await
            .unwrap();
            let mut stream = c
                .sync_regions(tokio_stream::wrappers::ReceiverStream::new(rx))
                .await
                .unwrap()
                .into_inner();
            loop {
                let f = timeout(Duration::from_secs(5), stream.message())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                if f.regions.is_empty() {
                    resume = f.start_index;
                    break;
                }
            }
            live = Some(stream);
            live_tx = Some(tx);
        }
        if event["name"] == "split-right" {
            drop(live.take());
            drop(live_tx.take());
        }
        let mut catchup = vec![];
        if event["name"] == "merge" {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            tx.send(pdpb::SyncRegionRequest {
                header: header.clone(),
                member: Some(pdpb::Member {
                    name: "theme53-reconnected".into(),
                    client_urls: vec!["http://127.0.0.1:29998".into()],
                    ..Default::default()
                }),
                start_index: resume,
            })
            .await
            .unwrap();
            let mut stream = c
                .sync_regions(tokio_stream::wrappers::ReceiverStream::new(rx))
                .await
                .unwrap()
                .into_inner();
            let mut frame = timeout(Duration::from_secs(5), stream.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            frame.header = None;
            catchup.push(hex(&frame.encode_to_vec()));
            drop(stream);
            drop(tx);
        }
        let snapshot = json!({"regions":regions,"store":hex(&raw_store.encode_to_vec()),"store_meta":hex(&store_meta.encode_to_vec()),"live":live_frames,"catchup":catchup,"hot_read":hot_read,"hot_write":hot_write,"buckets":buckets,"sync":frames,"kv":values,"error":error});
        println!("{} {}", event["name"], snapshot);
        snapshots.push(snapshot);
    }
    std::fs::write("tests/rust-compat/fixtures/state/task-53.json",serde_json::to_string_pretty(&json!({"baseline":"69ba16e6d47dfe59a21498c21641e1c1eaa726e0","events":events,"snapshots":snapshots})).unwrap()+"\n").unwrap();
}

async fn restart_snapshot(
    c: &mut PdClient<tonic::transport::Channel>,
    url: &str,
    header: Option<pdpb::RequestHeader>,
    id: u64,
) -> Value {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tx.send(pdpb::SyncRegionRequest {
        header,
        member: Some(pdpb::Member {
            name: "theme53-durability".into(),
            client_urls: vec!["http://127.0.0.1:29998".into()],
            ..Default::default()
        }),
        start_index: 0,
    })
    .await
    .unwrap();
    let mut stream = c
        .sync_regions(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let mut frames = vec![];
    let history_index;
    loop {
        let mut f = timeout(Duration::from_secs(5), stream.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let done = f.regions.is_empty();
        f.header = None;
        frames.push(hex(&f.encode_to_vec()));
        if done {
            history_index = f.start_index;
            break;
        }
    }
    drop(stream);
    drop(tx);
    let backend = EtcdMetaStore::connect(&[url.to_string()], Duration::from_secs(3))
        .await
        .unwrap();
    let read = backend
        .read(Read::prefix(
            format!("/pd/{id}/raft/r/").as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let values = read
        .entries
        .iter()
        .map(|e| {
            hex(&metapb::Region::decode(e.value.as_slice())
                .unwrap()
                .encode_to_vec())
        })
        .collect::<Vec<_>>();
    // RegionSyncer checkpoints use its independent region-storage Base (LevelDB),
    // even with use-region-storage=false. The completion index after a real process
    // restart is the observable persistence oracle; it is not an etcd revision/key.
    let checkpoint = history_index.to_string();
    json!({"sync":frames,"kv":values,"checkpoint":checkpoint})
}
async fn restart_phase(
    c: &mut PdClient<tonic::transport::Channel>,
    url: &str,
    header: Option<pdpb::RequestHeader>,
    id: u64,
    phase: &str,
) {
    let path = "tests/rust-compat/fixtures/state/task-53-restart.json";
    if phase == "checkpoint" {
        let mut requests = vec![];
        let mut inputs = vec![];
        for version in 5..=99 {
            let mut r = region(1, b"", b"", version, 3, 9);
            inputs.push(hex(&r.encode_to_vec()));
            r.header = header.clone();
            requests.push(r);
        }
        timeout(Duration::from_secs(15), async {
            let mut response = match c.region_heartbeat(tokio_stream::iter(requests)).await {
                Ok(response) => response.into_inner(),
                Err(e) if e.code() == tonic::Code::Unknown && e.message() == "EOF" => {
                    println!("Go heartbeat input exhausted: Unknown/EOF before headers");
                    return;
                }
                Err(e) => panic!("unexpected heartbeat initialization: {e}"),
            };
            loop {
                match response.message().await {
                    Ok(Some(message)) => assert!(message.header.unwrap().error.is_none()),
                    Ok(None) => break,
                    Err(e) if e.code() == tonic::Code::Unknown && e.message() == "EOF" => {
                        println!("Go heartbeat input exhausted: Unknown/EOF");
                        break;
                    }
                    Err(e) => panic!("unexpected heartbeat termination: {e}"),
                }
            }
        })
        .await
        .unwrap();
        let mut snapshot = Value::Null;
        for _ in 0..50 {
            snapshot = restart_snapshot(c, url, header.clone(), id).await;
            if snapshot["checkpoint"] == "100" {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(snapshot["checkpoint"], "100");
        std::fs::write(
            path,
            serde_json::to_string_pretty(&json!({"events":inputs,"before":snapshot})).unwrap()
                + "\n",
        )
        .unwrap();
        println!("checkpoint before restart: {snapshot}");
    } else {
        assert_eq!(phase, "restarted");
        let mut f: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let after = restart_snapshot(c, url, header.clone(), id).await;
        assert_eq!(after["checkpoint"], "100");
        assert_eq!(after["kv"], f["before"]["kv"]);
        println!("checkpoint after restart: {after}");
        f["after"] = after;
        let mut r = region(1, b"", b"", 100, 3, 10);
        f["next_event"] = json!(hex(&r.encode_to_vec()));
        r.header = header.clone();
        timeout(Duration::from_secs(5), async {
            let mut response = match c.region_heartbeat(tokio_stream::iter([r])).await {
                Ok(response) => response.into_inner(),
                Err(e) if e.code() == tonic::Code::Unknown && e.message() == "EOF" => {
                    println!("Go heartbeat input exhausted: Unknown/EOF before headers");
                    return;
                }
                Err(e) => panic!("unexpected heartbeat initialization: {e}"),
            };
            loop {
                match response.message().await {
                    Ok(Some(message)) => assert!(message.header.unwrap().error.is_none()),
                    Ok(None) => break,
                    Err(e) if e.code() == tonic::Code::Unknown && e.message() == "EOF" => {
                        println!("Go heartbeat input exhausted: Unknown/EOF");
                        break;
                    }
                    Err(e) => panic!("unexpected heartbeat termination: {e}"),
                }
            }
        })
        .await
        .unwrap();
        sleep(Duration::from_millis(100)).await;
        let advanced = restart_snapshot(c, url, header, id).await;
        println!("checkpoint after next report: {advanced}");
        f["advanced"] = advanced;
        std::fs::write(path, serde_json::to_string_pretty(&f).unwrap() + "\n").unwrap();
    }
}
