// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_ratelimit::ConcurrentRunner;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{HotStat, LabelStatistics, RegionStatistics};
use astersql_pd_statistics_buckets::{BucketFlows, BucketReport};
use astersql_pd_storage::Storage;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{Consistency, MemoryStore, MetaStore, Read};
use astersql_pd_syncer::*;
use prometheus::Registry;
use std::sync::{Arc, Mutex};
use tokio::{
    runtime::Handle,
    sync::mpsc,
    time::{Duration, timeout},
};
use tokio_util::sync::CancellationToken;

fn region(id: u64, start: &str, end: &str, version: u64) -> RegionInfo {
    let peers: Vec<_> = (1..=3)
        .map(|store_id| Peer {
            id: store_id + 10,
            store_id,
            ..Default::default()
        })
        .collect();
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.into(),
                end_key: end.into(),
            },
            epoch: Some(RegionEpoch {
                version,
                conf_ver: 2,
            }),
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    );
    r.set_source(RegionSource::Heartbeat);
    r.set_report_interval(0, 60);
    r.stats_mut().approximate_size = 10;
    r.stats_mut().approximate_keys = 100;
    r.stats_mut().written_bytes = 60_000_000;
    r.stats_mut().written_keys = 60_000;
    r
}
struct Sender(mpsc::Sender<SyncFrame>);
impl DownstreamSender for Sender {
    async fn send(&mut self, frame: SyncFrame) -> Result<(), String> {
        self.0.send(frame).await.map_err(|e| e.to_string())
    }
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
        for id in 1..=3 {
            cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                node_state: NodeState::Serving,
                ..Default::default()
            })));
        }
        let storage = Arc::new(StorageEndpoint::new(store));
        let paths = ClusterPaths::new(52);
        let mut config = ProviderSnapshot::default();
        config.replication.enable_placement_rules = false;
        config.schedule.max_merge_region_size = 20;
        let mut runners = std::array::from_fn(|_| ConcurrentRunner::new(None));
        for r in &mut runners {
            r.start(&CancellationToken::new());
        }
        Self {
            state: Arc::new(Mutex::new(HeartbeatEffectsState {
                cluster,
                region_stats: Some(RegionStatistics::new()),
                label_stats: LabelStatistics::new(),
                config,
                rules: RuleManager::new(storage.clone(), paths),
            })),
            hot: HotStat::new(&Handle::current(), &Registry::new(), 128).unwrap(),
            runners,
            storage,
            paths,
            downstreams: Downstreams::new(HistoryBuffer::new(100).unwrap()),
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
            bucket_enabled: true,
        }
    }
    async fn apply(&self, r: RegionInfo) -> RegionHeartbeatResult {
        let mut receipt = self.pipeline().region(r, 100, 100_000_000_000).unwrap();
        assert!(
            timeout(Duration::from_secs(5), receipt.wait())
                .await
                .unwrap()
                .is_empty()
        );
        receipt.heartbeat
    }
    async fn stop(&mut self) {
        for r in &mut self.runners {
            r.stop().await;
        }
        self.hot.shutdown().await.unwrap();
    }
}
fn response(frame: &SyncFrame) -> SyncApplyResponse {
    let mut out = SyncApplyResponse {
        start_index: frame.start_index(),
        ..Default::default()
    };
    for r in frame.records() {
        out.regions.push(r.meta().clone());
        out.leaders.push(r.leader().cloned());
        let s = r.stats();
        out.stats.push(SyncRegionStats {
            written_bytes: s.written_bytes,
            written_keys: s.written_keys,
            read_bytes: s.read_bytes,
            read_keys: s.read_keys,
        });
        out.buckets.push(r.buckets().map(|b| RegionBucketMeta {
            version: b.version,
            keys: b.keys,
        }));
    }
    out
}
async fn replay(store: Arc<dyn MetaStore>) {
    let mut h = Host::new(store.clone());
    let (tx, mut rx) = mpsc::channel(16);
    let task = h.downstreams.bind(
        &Handle::current(),
        "replica".into(),
        HistoryIndex(0),
        Sender(tx),
    );
    let replica_store = Arc::new(MemoryStore::default());
    let replica_storage = Storage::new(replica_store.clone());
    let mut replica = BasicCluster::new();
    let mut client = ClientApply::default();
    let mut history = HistoryBuffer::new(100).unwrap();
    // Completion establishes incremental mode without manufacturing region results.
    client
        .apply(
            SyncApplyResponse::default(),
            &mut replica,
            &replica_storage,
            &h.paths,
            &mut history,
        )
        .await
        .unwrap();
    let sequence = [
        region(1, "", "", 2),
        region(1, "", "", 3),
        region(2, "m", "", 4),
        region(1, "", "m", 4),
        region(1, "", "", 5),
    ];
    for (i, r) in sequence.into_iter().enumerate() {
        let accepted = h.apply(r.clone()).await;
        assert!(accepted.guide.save_kv);
        let frame = timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.start_index(), HistoryIndex(i as u64));
        let outcome = client
            .apply(
                response(&frame),
                &mut replica,
                &replica_storage,
                &h.paths,
                &mut history,
            )
            .await
            .unwrap();
        assert_eq!(outcome.applied, 1);
        assert_eq!(
            (outcome.stale, outcome.save_errors, outcome.delete_errors),
            (0, 0, 0)
        );
        let snapshot = h.pipeline().snapshot().await.unwrap();
        assert_eq!(snapshot.classifications.len(), snapshot.regions.len());
        assert_eq!(snapshot.store_regions[0].1.region, snapshot.regions.len());
        assert_eq!(snapshot.store_regions[0].1.leader, snapshot.regions.len());
        assert!(snapshot.labels.is_empty());
        let mut follower = replica.regions.get_regions();
        follower.sort_by_key(|r| r.meta().id);
        assert_eq!(snapshot.regions.len(), follower.len());
        for (local, remote) in snapshot.regions.iter().zip(&follower) {
            assert_eq!(local.meta(), remote.meta());
            assert_eq!(local.leader(), remote.leader());
            assert_eq!(local.stats().written_bytes, remote.stats().written_bytes);
            // Go followers consume BucketMeta, not the source's raw report counters/period.
            let bucket_meta =
                |r: &RegionInfo| r.buckets().map(|b| (b.region_id, b.version, b.keys));
            assert_eq!(bucket_meta(local), bucket_meta(remote));
            assert_eq!(local.get_ref(), 2);
            assert_eq!(
                h.storage
                    .load_region(&h.paths, local.meta().id)
                    .await
                    .unwrap(),
                Some(local.meta().clone())
            );
            assert_eq!(
                replica_storage
                    .load_region(&h.paths, remote.meta().id)
                    .await
                    .unwrap(),
                Some(remote.meta().clone())
            );
        }
        assert_eq!(history.next_index(), HistoryIndex(i as u64 + 1));
        // Duplicate and rejected reports must preserve storage/history and root identity.
        let revision = store
            .read(Read::prefix(b"", Consistency::Linearizable))
            .await
            .unwrap()
            .revision;
        let root = h
            .state
            .lock()
            .unwrap()
            .cluster
            .regions
            .get_region(r.meta().id)
            .unwrap();
        assert_eq!(h.apply(r.clone()).await.guide, RegionGuide::default());
        assert!(
            h.pipeline()
                .region(region(r.meta().id, "", "", 1), 100, 100_000_000_000)
                .is_err()
        );
        assert!(Arc::ptr_eq(
            &root,
            &h.state
                .lock()
                .unwrap()
                .cluster
                .regions
                .get_region(r.meta().id)
                .unwrap()
        ));
        assert_eq!(
            store
                .read(Read::prefix(b"", Consistency::Linearizable))
                .await
                .unwrap()
                .revision,
            revision
        );
        assert_eq!(h.downstreams.records_from(HistoryIndex(0)).len(), i + 1);
        assert!(rx.try_recv().is_err());
        if i == 0 {
            let report = BucketReport {
                region_id: 1,
                version: 2,
                keys: vec![b"a".to_vec(), b"z".to_vec()],
                period_in_ms: 1000,
                flows: BucketFlows {
                    read_bytes: vec![100000],
                    read_keys: vec![0],
                    read_qps: vec![0],
                    write_bytes: vec![0],
                    write_keys: vec![0],
                    write_qps: vec![0],
                },
            };
            assert!(h.pipeline().bucket(report.clone()).unwrap());
            assert!(h.pipeline().bucket(report).unwrap());
            let snap = h.pipeline().snapshot().await.unwrap();
            assert_eq!(snap.regions[0].buckets().unwrap().version, 2);
            assert!(snap.statistics.buckets.contains_key(&1));
            assert!(replica.regions.get_region(1).unwrap().buckets().is_none());
        }
    }
    assert_eq!(replica.regions.get_regions().len(), 1);
    assert!(replica.regions.get_region(2).is_none());
    assert!(h.storage.load_region(&h.paths, 2).await.unwrap().is_none());
    assert!(
        replica_storage
            .load_region(&h.paths, 2)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !h.state
            .lock()
            .unwrap()
            .region_stats
            .as_ref()
            .unwrap()
            .is_observed(2)
    );
    task.cancel();
    assert!(task.join().await.unwrap().is_ok());
    assert!(h.downstreams.names().is_empty());
    h.stop().await;
}
#[tokio::test]
async fn state_task_52_state_pipeline_memory_replay() {
    replay(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn state_task_52_state_pipeline_etcd_replay() {
    let fixture = effects_fixture::Fixture::start();
    replay(Arc::new(fixture.store().await)).await;
}
#[tokio::test]
async fn state_task_52_state_pipeline_hot_flow_and_owned_snapshots() {
    let mut h = Host::new(Arc::new(MemoryStore::default()));
    let r = region(1, "", "", 2);
    for _ in 0..3 {
        h.apply(r.clone()).await;
        let report = StoreStats {
            store_id: 1,
            capacity: 100,
            available: 50,
            interval: Some(TimeInterval {
                start_timestamp: 0,
                end_timestamp: 60,
            }),
            peer_stats: vec![PeerStat {
                region_id: 1,
                read_bytes: 60_000_000,
                read_keys: 60_000,
                ..Default::default()
            }],
            ..Default::default()
        };
        h.pipeline().store(&report, 100_000_000_000).unwrap();
        h.pipeline().snapshot().await.unwrap();
    }
    let mut snapshot = h.pipeline().snapshot().await.unwrap();
    let before = format!("{:?}", snapshot.statistics);
    assert!(
        h.pipeline()
            .region(region(1, "", "", 1), 100, 100_000_000_000)
            .is_err()
    );
    assert_eq!(
        h.pipeline().store(
            &StoreStats {
                store_id: 99,
                ..Default::default()
            },
            100
        ),
        Err(StoreNotFound(99))
    );
    assert_eq!(
        format!("{:?}", h.pipeline().snapshot().await.unwrap().statistics),
        before
    );
    h.stop().await;
    assert!(
        h.pipeline().snapshot().await.is_err(),
        "closed workers must not publish a partial snapshot"
    );
    assert_eq!(
        snapshot.statistics.write.get(&1).map_or(0, Vec::len),
        1,
        "accepted region reports must feed canonical write statistics"
    );
    assert_eq!(snapshot.statistics.read.get(&1).map_or(0, Vec::len), 1);
    assert_eq!(snapshot.stores[0].1.capacity, 100);
    snapshot.stores[0].1.capacity = 999;
    assert_eq!(
        h.state
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(1)
            .unwrap()
            .store_stats()
            .capacity,
        100
    );
}

#[tokio::test]
async fn task_127_pipeline_same_view_duplicate_stale_and_stopped_hot() {
    let mut h = Host::new(Arc::new(MemoryStore::default()));
    let (tx, rx) = std::sync::mpsc::sync_channel(4);
    let original = region(127, "", "", 2);
    let mut meta = original.meta().clone();
    meta.peers[0].is_witness = true;
    let mut r = RegionInfo::new(meta.clone(), Some(meta.peers[0].clone()), vec![], vec![]);
    *r.stats_mut() = original.stats().clone();
    r.set_report_interval(0, 60);
    r.set_source(RegionSource::Heartbeat);
    for _ in 0..2 {
        let mut receipt = h
            .pipeline()
            .region_with_witness(r.clone(), 100, 0, Some(&tx))
            .unwrap();
        let event = rx.try_recv().unwrap();
        assert!(Arc::ptr_eq(&event, &receipt.heartbeat.region));
        assert_eq!(event.leader().unwrap().is_witness, true);
        assert!(receipt.wait().await.is_empty());
    }
    assert!(
        h.pipeline()
            .region_with_witness(region(127, "", "", 1), 100, 0, Some(&tx))
            .is_err()
    );
    assert!(rx.try_recv().is_err());
    assert_eq!(
        h.pipeline()
            .snapshot()
            .await
            .unwrap()
            .statistics
            .write
            .get(&2)
            .map_or(0, Vec::len),
        1
    );
    // A newer ordinary report updates the root but does not enqueue recovery.
    h.apply(region(127, "", "", 3)).await;
    assert!(rx.try_recv().is_err());
    assert!(
        !h.state
            .lock()
            .unwrap()
            .cluster
            .regions
            .get_region(127)
            .unwrap()
            .leader()
            .unwrap()
            .is_witness
    );
    h.hot.shutdown().await.unwrap();
    meta.epoch.as_mut().unwrap().version = 4;
    let next = RegionInfo::new(meta.clone(), Some(meta.peers[0].clone()), vec![], vec![]);
    let mut receipt = h
        .pipeline()
        .region_with_witness(next, 100, 0, Some(&tx))
        .unwrap();
    assert_eq!(
        rx.try_recv()
            .unwrap()
            .meta()
            .epoch
            .as_ref()
            .unwrap()
            .version,
        4
    );
    assert!(receipt.wait().await.is_empty());
    h.stop().await;
}
