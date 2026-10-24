// Copyright 2026 AsterSQL.
//! State-only assembly and strict observed-field comparison; no business algorithms.
use astersql_pd_cluster::*;
use astersql_pd_core::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{metapb, pdpb};
use astersql_pd_ratelimit::ConcurrentRunner;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_server::{
    bucket_heartbeat_adapter::bucket_from_proto,
    core_region_adapter::{region_from_heartbeat, region_to_proto},
    core_store_adapter::{store_stats_from_proto, store_stats_to_proto, store_to_proto},
    sync_codec::*,
};
use astersql_pd_statistics::{HotStat, LabelStatistics, RegionStatistics, get_hot_status};
use astersql_pd_statistics_utils::RWType;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MetaStore;
use astersql_pd_syncer::*;
use prometheus::Registry;
use prost::Message;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{runtime::Handle, time::timeout};
use tokio_util::sync::CancellationToken;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
pub fn unhex(v: &Value) -> Vec<u8> {
    let s = v.as_str().unwrap();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
pub fn fixture() -> Value {
    let f: Value = serde_json::from_str(include_str!("fixtures/state/task-53.json")).unwrap();
    assert_eq!(f["baseline"], "69ba16e6d47dfe59a21498c21641e1c1eaa726e0");
    assert!(
        f["events"].as_array().unwrap().len() >= 16,
        "empty/incomplete oracle"
    );
    assert_eq!(
        f["events"].as_array().unwrap().len(),
        f["snapshots"].as_array().unwrap().len()
    );
    f
}
pub fn compare(expected: &Value, actual: &Value) -> Result<(), String> {
    let d = crate::normalize::differences(expected, actual);
    if d.is_empty() {
        Ok(())
    } else {
        Err(d.join("\n"))
    }
}
fn peer(p: &Peer) -> Value {
    let mut v = json!({"id":p.id,"store_id":p.store_id,"role_name":"Voter"});
    assert_eq!(p.role, PeerRole::Voter);
    if p.is_witness {
        v["is_witness"] = json!(true)
    }
    v
}
fn region_json(r: &RegionInfo) -> Value {
    let s = r.stats();
    let e = r.meta().epoch.as_ref().unwrap();
    let mut v = json!({"id":r.meta().id,"start_key":hex(&r.meta().key_range.start_key).to_uppercase(),"end_key":hex(&r.meta().key_range.end_key).to_uppercase(),"epoch":{"conf_ver":e.conf_ver,"version":e.version},"peers":r.meta().peers.iter().map(peer).collect::<Vec<_>>(),"leader":r.leader().map(peer).unwrap_or(json!({"role_name":"Voter"})),"cpu_usage":r.response_cpu_usage(),"read_bytes":s.read_bytes,"read_keys":s.read_keys,"written_bytes":s.written_bytes,"written_keys":s.written_keys,"approximate_size":s.approximate_size,"approximate_keys":s.approximate_keys,"approximate_kv_size":s.approximate_kv_size,"approximate_columnar_kv_size":s.approximate_columnar_kv_size});
    assert!(r.down_peers().is_empty() && r.pending_peers().is_empty());
    if let Some(b) = r.buckets() {
        v["bucket_version"] = json!(b.version);
        v["buckets"] = json!(
            b.keys
                .iter()
                .map(|k| hex(k).to_uppercase())
                .collect::<Vec<_>>()
        );
    }
    v
}
fn canonical_sync(frames: Vec<pdpb::SyncRegionResponse>) -> Value {
    let mut out = vec![];
    for mut f in frames {
        f.header = None;
        let mut records = vec![];
        for (i, r) in f.regions.iter().enumerate() {
            records.push((r.id,json!({"meta":hex(&r.encode_to_vec()),"stats":f.region_stats.get(i).map(|x|hex(&x.encode_to_vec())),"leader":f.region_leaders.get(i).map(|x|hex(&x.encode_to_vec())),"bucket":f.buckets.get(i).map(|x|hex(&x.encode_to_vec()))})));
        }
        records.sort_by_key(|r| r.0);
        out.push(json!({"index":f.start_index,"records":records.into_iter().map(|r|r.1).collect::<Vec<_>>(),"down_peers":format!("{:?}",f.down_peers),"pending_peers":format!("{:?}",f.pending_peers)}));
    }
    json!(out)
}
/// Normalize only map iteration order, equivalent JSON numeric tokens, and host time origin.
pub fn normalize(mut s: Value) -> Value {
    for key in ["sync", "live", "catchup"] {
        if s[key][0].is_string() || s[key].as_array().is_some_and(Vec::is_empty) {
            s[key] = canonical_sync(
                s[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| pdpb::SyncRegionResponse::decode(unhex(v).as_slice()).unwrap())
                    .collect(),
            );
        }
    }
    for key in ["hot_read", "hot_write"] {
        for kind in ["as_peer", "as_leader"] {
            if let Some(stores) = s[key][kind].as_object_mut() {
                for store in stores.values_mut() {
                    if let Some(peers) = store["statistics"].as_array_mut() {
                        peers.sort_by_key(|p| p["region_id"].as_u64().unwrap());
                    }
                }
            }
        }
    }
    // JSON has one numeric value domain; do not introduce a float tolerance.
    fn numbers(v: &mut Value) {
        match v {
            Value::Number(n) => {
                if let Some(x) = n.as_f64() {
                    if x.fract() == 0.0 && x.abs() < (1u64 << 53) as f64 {
                        *v = json!(x as i64)
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(numbers),
            Value::Object(o) => o.values_mut().for_each(numbers),
            _ => {}
        }
    }
    numbers(&mut s);
    s
}
struct Host {
    state: Arc<Mutex<HeartbeatEffectsState<ProviderSnapshot<()>>>>,
    hot: HotStat,
    runners: [ConcurrentRunner; 3],
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    downstreams: Downstreams,
}
impl Host {
    fn new(store: Arc<dyn MetaStore>) -> Self {
        let cluster = BasicCluster::new();
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: 1,
            node_state: NodeState::Preparing,
            address: "127.0.0.1:29999".into(),
            version: "8.5.0".into(),
            ..Default::default()
        })));
        let storage = Arc::new(StorageEndpoint::new(store));
        let paths = ClusterPaths::new(53);
        let mut config = ProviderSnapshot::default();
        config.replication.enable_placement_rules = false;
        config.replication.max_replicas = 1;
        let mut runners = std::array::from_fn(|_| ConcurrentRunner::new(None));
        for r in &mut runners {
            r.start(&CancellationToken::new());
        }
        let hot = HotStat::new(&Handle::current(), &Registry::new(), 128).unwrap();
        hot.stores_stats.get_or_create_rolling_store_stats(1);
        Self {
            state: Arc::new(Mutex::new(HeartbeatEffectsState {
                cluster,
                region_stats: Some(RegionStatistics::new()),
                label_stats: LabelStatistics::new(),
                config,
                rules: RuleManager::new(storage.clone(), paths),
            })),
            hot,
            runners,
            storage,
            paths,
            downstreams: Downstreams::new(HistoryBuffer::new(10000).unwrap()),
        }
    }
    fn pipeline(&self) -> StatePipeline<'_, ProviderSnapshot<()>> {
        StatePipeline {
            state: &self.state,
            hot: &self.hot,
            runners: HeartbeatRunners {
                subtree: &self.runners[0],
                misc: &self.runners[1],
                sync: &self.runners[2],
            },
            storage: self.storage.clone(),
            paths: self.paths,
            downstreams: self.downstreams.clone(),
            bucket_enabled: false,
        }
    }
    async fn event(&self, e: &Value) -> Option<Value> {
        let bytes = unhex(&e["wire"]);
        match e["kind"].as_str().unwrap() {
            "region" => {
                let r = region_from_heartbeat(
                    pdpb::RegionHeartbeatRequest::decode(bytes.as_slice()).unwrap(),
                    1,
                )
                .unwrap();
                match self.pipeline().region(r, 100, 100_000_000_000) {
                    Ok(mut receipt) => {
                        assert!(
                            timeout(Duration::from_secs(5), receipt.wait())
                                .await
                                .unwrap()
                                .is_empty()
                        );
                        None
                    }
                    Err(e) => Some(
                        json!({"type":1,"incoming":hex(&region_to_proto(e.incoming).encode_to_vec()),"current":hex(&region_to_proto(e.current).encode_to_vec())}),
                    ),
                }
            }
            "store" => {
                self.pipeline()
                    .store(
                        &store_stats_from_proto(Some(
                            pdpb::StoreStats::decode(bytes.as_slice()).unwrap(),
                        ))
                        .unwrap(),
                        100_000_000_000,
                    )
                    .unwrap();
                None
            }
            "bucket" => {
                assert!(
                    self.pipeline()
                        .bucket(bucket_from_proto(
                            metapb::Buckets::decode(bytes.as_slice()).unwrap()
                        ))
                        .unwrap()
                );
                None
            }
            _ => panic!("unknown event"),
        }
    }
    async fn snapshot(&self, error: Option<Value>) -> Value {
        let s = self.pipeline().snapshot().await.unwrap();
        let state = self.state.lock().unwrap();
        let stores = state
            .cluster
            .stores
            .get_stores()
            .into_iter()
            .map(|s| s.as_ref().clone())
            .collect::<Vec<_>>();
        let hot = |rw,
                   _peers: &std::collections::BTreeMap<
            u64,
            Vec<astersql_pd_statistics::HotPeerStat>,
        >| {
            let peers: HashMap<_, _> = s.statistics.get_hot_peer_stats(rw, 0).into_iter().collect();
            let mut v = serde_json::to_value(get_hot_status(
                &stores,
                &s.statistics.stores,
                &peers,
                rw,
                true,
            ))
            .unwrap();
            for kind in ["as_peer", "as_leader"] {
                for store in v[kind].as_object_mut().unwrap().values_mut() {
                    if let Some(peers) = store["statistics"].as_array_mut() {
                        for peer in peers {
                            let r = s
                                .regions
                                .iter()
                                .find(|r| r.meta().id == peer["region_id"].as_u64().unwrap());
                            peer["last_update_time"] = json!(if rw == RWType::READ {
                                "<store-heartbeat-time>"
                            } else if let Some(r) = r {
                                assert_eq!(r.report_interval(), (0, 60));
                                "1970-01-01T08:01:00+08:00"
                            } else {
                                "0001-01-01T00:00:00Z"
                            });
                        }
                    }
                }
            }
            let _ = peers;
            v
        };
        let (frames, _) = self
            .downstreams
            .prepare_sync(&state.cluster, HistoryIndex(0))
            .unwrap();
        let frames = frames
            .into_iter()
            .map(|f| match f {
                BootstrapFrame::Snapshot((offset, records)) => snapshot_to_proto(
                    0,
                    SnapshotBatch {
                        offset: SnapshotOffset(offset as usize),
                        records: &records,
                    },
                ),
                BootstrapFrame::History(f) => history_to_proto(0, &f),
            })
            .collect();
        let mut store_meta = state
            .cluster
            .stores
            .get_store(1)
            .unwrap()
            .meta()
            .unwrap()
            .clone();
        store_meta.last_heartbeat = 0;
        drop(state);
        let mut kv = vec![];
        for id in [1, 2] {
            if let Some(meta) = self.storage.load_region(&self.paths, id).await.unwrap() {
                kv.push((id, hex(&region_to_proto(meta).encode_to_vec())));
            }
        }
        let buckets=s.statistics.buckets.iter().map(|(id,stats)|(id.to_string(),json!(stats.iter().map(|b|json!({"start_key":hex(&b.start_key).to_uppercase(),"end_key":hex(&b.end_key).to_uppercase(),"hot_degree":b.hot_degree,"read_bytes":b.loads[0],"read_keys":b.loads[1],"write_bytes":b.loads[3],"write_keys":b.loads[4]})).collect::<Vec<_>>()))).collect::<serde_json::Map<_,_>>();
        json!({"regions":{"count":s.regions.len(),"regions":s.regions.iter().map(|r|region_json(r)).collect::<Vec<_>>()},"store":hex(&store_stats_to_proto(s.stores[0].1.clone()).encode_to_vec()),"store_meta":hex(&store_to_proto(store_meta).encode_to_vec()),"hot_read":hot(RWType::READ,&s.statistics.read),"hot_write":hot(RWType::WRITE,&s.statistics.write),"buckets":buckets,"sync":canonical_sync(frames),"kv":kv,"error":error})
    }
    async fn stop(&mut self) {
        for r in &mut self.runners {
            r.stop().await;
        }
        self.hot.shutdown().await.unwrap();
        assert!(self.pipeline().snapshot().await.is_err());
    }
}
pub async fn replay(
    store: Arc<dyn MetaStore>,
    restart: Option<&mut crate::harness::fixture::Fixture>,
) {
    let f = fixture();
    let mut h = Host::new(store.clone());
    let mut comparisons = vec![];
    struct Sender(tokio::sync::mpsc::Sender<SyncFrame>);
    impl DownstreamSender for Sender {
        async fn send(&mut self, frame: SyncFrame) -> Result<(), String> {
            self.0.send(frame).await.map_err(|e| e.to_string())
        }
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel::<SyncFrame>(16);
    let mut live_task = None;
    let mut resume = HistoryIndex(0);
    for (event, expected) in f["events"]
        .as_array()
        .unwrap()
        .iter()
        .zip(f["snapshots"].as_array().unwrap())
    {
        let error = h.event(event).await;
        let mut actual = h.snapshot(error).await;
        let mut live = vec![];
        if live_task.is_some() {
            while let Ok(Some(frame)) = timeout(Duration::from_millis(80), rx.recv()).await {
                resume = HistoryIndex(frame.start_index().0 + frame.records().len() as u64);
                live.push(history_to_proto(0, &frame));
            }
        }
        actual["live"] = canonical_sync(live);
        if event["name"] == "initial" {
            let (_, index) = h
                .downstreams
                .prepare_sync(&h.state.lock().unwrap().cluster, HistoryIndex(0))
                .unwrap();
            resume = index;
            live_task = Some(h.downstreams.bind(
                &Handle::current(),
                "theme-live".into(),
                index,
                Sender(tx.clone()),
            ));
        }
        if event["name"] == "split-right" {
            let task = live_task.take().unwrap();
            task.cancel();
            assert!(task.join().await.unwrap().is_ok());
            assert!(h.downstreams.names().is_empty());
        }
        let mut catchup = vec![];
        if event["name"] == "merge" {
            let (frames, _) = h
                .downstreams
                .prepare_sync(&h.state.lock().unwrap().cluster, resume)
                .unwrap();
            for f in frames {
                match f {
                    BootstrapFrame::History(frame) => catchup.push(history_to_proto(0, &frame)),
                    _ => panic!("reconnect unexpectedly required full snapshot"),
                }
            }
        }
        actual["catchup"] = canonical_sync(catchup);
        let mut expected = expected.clone();
        if !expected["error"].is_null() {
            // Preserve the original Go message in the fixture; compare typed stale metadata.
            // Only the three runtime errors from this exact trace may enter this projection.
            assert!(
                expected["error"]["message"]
                    .as_str()
                    .unwrap()
                    .starts_with("region is stale: region ")
            );
            let request =
                pdpb::RegionHeartbeatRequest::decode(unhex(&event["wire"]).as_slice()).unwrap();
            let frame =
                pdpb::SyncRegionResponse::decode(unhex(&expected["sync"][0]).as_slice()).unwrap();
            let current = frame
                .regions
                .into_iter()
                .find(|r| r.id == request.region.as_ref().unwrap().id)
                .unwrap();
            expected["error"] = json!({"type":expected["error"]["type"],"incoming":hex(&request.region.unwrap().encode_to_vec()),"current":hex(&current.encode_to_vec())});
        }
        comparisons.push((
            event["name"].clone(),
            normalize(expected),
            normalize(actual),
        ));
    }
    if let Some(task) = live_task {
        task.cancel();
        assert!(task.join().await.unwrap().is_ok());
    }
    drop(tx);
    let restart_fixture: Value =
        serde_json::from_str(include_str!("fixtures/state/task-53-restart.json")).unwrap();
    assert_eq!(restart_fixture["events"].as_array().unwrap().len(), 95);
    for wire in restart_fixture["events"].as_array().unwrap() {
        assert!(
            h.event(&json!({"kind":"region","wire":wire}))
                .await
                .is_none()
        );
    }
    let before = h.snapshot(None).await;
    let mut persisted = PersistentHistory::load(HistoryBuffer::new(10000).unwrap(), store.as_ref())
        .await
        .unwrap();
    for record in h.downstreams.records_from(HistoryIndex(0)) {
        persisted.record(record).await.unwrap();
    }
    assert_eq!(persisted.history().next_index(), HistoryIndex(100));
    assert_eq!(
        astersql_pd_storage_kv::Base::new(store.as_ref())
            .load(HISTORY_KEY)
            .await
            .unwrap(),
        b"100"
    );
    drop(persisted);
    h.stop().await;
    let restored: Arc<dyn MetaStore> = if let Some(f) = restart {
        f.command("stop", "stopped");
        f.command("start", "restarted");
        Arc::new(f.store().await)
    } else {
        store
    };
    let mut h = Host::new(restored.clone());
    let meta = h.storage.load_region(&h.paths, 1).await.unwrap().unwrap();
    h.state
        .lock()
        .unwrap()
        .cluster
        .regions
        .put_region(Arc::new(RegionInfo::new(meta, None, vec![], vec![])));
    let mut history = HistoryBuffer::new(10000).unwrap();
    history.reset_with_index(HistoryIndex(100));
    h.downstreams = Downstreams::new(history);
    let mut persisted =
        PersistentHistory::load(HistoryBuffer::new(10000).unwrap(), restored.as_ref())
            .await
            .unwrap();
    assert_eq!(persisted.history().next_index(), HistoryIndex(100));
    assert!(persisted.history().is_empty());
    let after = h.snapshot(None).await;
    assert!(
        h.event(&json!({"kind":"region","wire":restart_fixture["next_event"]}))
            .await
            .is_none()
    );
    for record in h.downstreams.records_from(HistoryIndex(100)) {
        persisted.record(record).await.unwrap();
    }
    assert_eq!(persisted.history().next_index(), HistoryIndex(101));
    let advanced = h.snapshot(None).await;
    h.stop().await;
    for (stage, actual) in [("before", before), ("after", after), ("advanced", advanced)] {
        let expected = &restart_fixture[stage];
        let expected_frames = canonical_sync(
            expected["sync"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| pdpb::SyncRegionResponse::decode(unhex(f).as_slice()).unwrap())
                .collect(),
        );
        assert_eq!(
            actual["sync"], expected_frames,
            "restart {stage}: complete source frames"
        );
        assert_eq!(
            actual["kv"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x[1].clone())
                .collect::<Vec<_>>(),
            expected["kv"].as_array().unwrap().clone(),
            "restart {stage}: complete persisted RegionMeta"
        );
    }
    for (name, expected, actual) in comparisons {
        assert!(
            compare(&expected, &actual).is_ok(),
            "{name}: {}",
            compare(&expected, &actual).unwrap_err()
        );
    }
}
