// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_ratelimit::ConcurrentRunner;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{LabelStatistics, RegionStatisticType, RegionStatistics};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{MemoryStore, MetaStore};
use astersql_pd_syncer::{Downstreams, HistoryBuffer, HistoryIndex};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

fn region(id: u64, start: &str, end: &str, version: u64) -> RegionInfo {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
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
    r.stats_mut().approximate_size = 10;
    r.stats_mut().approximate_keys = 100;
    r
}
struct Host {
    state: Arc<Mutex<HeartbeatEffectsState<ProviderSnapshot<()>>>>,
    endpoint: Arc<StorageEndpoint>,
    history: Downstreams,
    subtree: ConcurrentRunner,
    misc: ConcurrentRunner,
    sync: ConcurrentRunner,
    paths: ClusterPaths,
}
impl Host {
    fn new(store: Arc<dyn MetaStore>) -> Self {
        let paths = ClusterPaths::new(46);
        let endpoint = Arc::new(StorageEndpoint::new(store));
        let mut config = ProviderSnapshot::default();
        config.replication.enable_placement_rules = false;
        config.schedule.max_merge_region_size = 20;
        config.schedule.max_merge_region_keys = 200000;
        let cluster = BasicCluster::new();
        for id in 1..=3 {
            cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            })));
        }
        let mut host = Self {
            state: Arc::new(Mutex::new(HeartbeatEffectsState {
                cluster,
                config,
                region_stats: Some(RegionStatistics::new()),
                label_stats: LabelStatistics::new(),
                affinity: None,
                rules: RuleManager::new(endpoint.clone(), paths),
            })),
            endpoint,
            history: Downstreams::new(HistoryBuffer::new(100).unwrap()),
            subtree: ConcurrentRunner::new(None),
            misc: ConcurrentRunner::new(None),
            sync: ConcurrentRunner::new(None),
            paths,
        };
        let cancel = CancellationToken::new();
        host.subtree.start(&cancel);
        host.misc.start(&cancel);
        host.sync.start(&cancel);
        host
    }
    fn submit(&self, r: RegionInfo) -> HeartbeatEffectsReceipt {
        submit_region_heartbeat(
            &self.state,
            r,
            false,
            &HeartbeatRunners {
                subtree: &self.subtree,
                misc: &self.misc,
                sync: &self.sync,
            },
            Some(self.endpoint.clone()),
            self.paths,
            Some(self.history.clone()),
            100,
        )
        .unwrap()
    }
    async fn apply(&self, r: RegionInfo) -> RegionHeartbeatResult {
        let mut receipt = self.submit(r);
        let failures = tokio::time::timeout(std::time::Duration::from_secs(10), receipt.wait())
            .await
            .unwrap();
        assert!(failures.is_empty(), "{failures:?}");
        receipt.heartbeat
    }
    async fn stop(&mut self) {
        self.subtree.stop().await;
        self.misc.stop().await;
        self.sync.stop().await;
    }
    fn cached(&self, id: u64) -> Arc<RegionInfo> {
        self.state
            .lock()
            .unwrap()
            .cluster
            .regions
            .get_region(id)
            .unwrap()
    }
    fn records(&self) -> Vec<Arc<RegionInfo>> {
        self.history.records_from(HistoryIndex(0))
    }
}
#[tokio::test]
async fn state_task_46_heartbeat_effects_guide_snapshots() {
    let mut h = Host::new(Arc::new(MemoryStore::default()));
    let mut r = region(1, "", "", 2);
    let result = h.apply(r.clone()).await;
    assert_eq!(
        result.guide,
        RegionGuide {
            save_kv: true,
            save_cache: true,
            need_sync: false,
            retained: true
        }
    );
    assert_eq!(h.cached(1).get_ref(), 2);
    assert_eq!(
        h.endpoint.load_region(&h.paths, 1).await.unwrap(),
        Some(r.meta().clone())
    );
    assert_eq!(h.records().len(), 1);
    let cached = h.cached(1);
    assert_eq!(h.apply(r.clone()).await.guide, RegionGuide::default());
    assert!(Arc::ptr_eq(&cached, &h.cached(1)));
    assert_eq!(h.records().len(), 1);
    let mut next = RegionInfo::new(
        r.meta().clone(),
        Some(r.meta().peers[1].clone()),
        vec![],
        vec![],
    );
    *next.stats_mut() = r.stats().clone();
    next.set_source(RegionSource::Heartbeat);
    r = next;
    let g = h.apply(r.clone()).await.guide;
    assert_eq!(
        g,
        RegionGuide {
            save_cache: true,
            need_sync: true,
            ..Default::default()
        }
    );
    assert_eq!(h.records().len(), 2);
    r.stats_mut().approximate_size = 144;
    assert_eq!(
        h.apply(r.clone()).await.guide,
        RegionGuide {
            save_cache: true,
            ..Default::default()
        }
    );
    assert_eq!(h.records().len(), 2);
    assert!(
        h.state
            .lock()
            .unwrap()
            .region_stats
            .as_ref()
            .unwrap()
            .is_region_stats_type(1, RegionStatisticType::OversizedRegion)
    );
    r.stats_mut().written_bytes = 24000;
    assert!(h.apply(r.clone()).await.guide.need_sync);
    assert_eq!(h.records().len(), 3);
    let mut flash = r.meta().clone();
    flash.is_in_flashback = true;
    flash.flashback_start_ts = 1;
    // Rebuild only metadata, retaining leader and statistics.
    let mut f = RegionInfo::new(flash, r.leader().cloned(), vec![], vec![]);
    *f.stats_mut() = r.stats().clone();
    f.set_source(RegionSource::Heartbeat);
    let g = h.apply(f.clone()).await.guide;
    assert!(g.save_cache && !g.save_kv && !g.need_sync);
    assert!(
        !h.endpoint
            .load_region(&h.paths, 1)
            .await
            .unwrap()
            .unwrap()
            .is_in_flashback
    );
    assert_eq!(h.records().len(), 3);
    h.stop().await;
}

#[tokio::test]
async fn state_task_46_heartbeat_effects_duplicate_config_and_root_only() {
    let mut h = Host::new(Arc::new(MemoryStore::default()));
    let r = region(1, "", "", 2);
    h.state
        .lock()
        .unwrap()
        .cluster
        .regions
        .check_and_put_root_tree(Arc::new(r.clone()))
        .unwrap();
    assert_eq!(h.cached(1).get_ref(), 1);
    let result = h.apply(r.clone()).await;
    assert_eq!(result.guide, RegionGuide::default());
    assert_eq!(h.cached(1).get_ref(), 2);
    assert!(h.endpoint.load_region(&h.paths, 1).await.unwrap().is_none());
    assert!(h.records().is_empty());
    assert!(
        h.state
            .lock()
            .unwrap()
            .region_stats
            .as_ref()
            .unwrap()
            .is_region_stats_type(1, RegionStatisticType::UndersizedRegion)
    );
    h.state
        .lock()
        .unwrap()
        .config
        .schedule
        .max_merge_region_size = 5;
    assert_eq!(h.apply(r.clone()).await.guide, RegionGuide::default());
    assert!(
        !h.state
            .lock()
            .unwrap()
            .region_stats
            .as_ref()
            .unwrap()
            .is_region_stats_type(1, RegionStatisticType::UndersizedRegion)
    );
    h.state
        .lock()
        .unwrap()
        .config
        .schedule
        .max_merge_region_size = 20;
    h.apply(r).await;
    assert!(
        h.state
            .lock()
            .unwrap()
            .region_stats
            .as_ref()
            .unwrap()
            .is_region_stats_type(1, RegionStatisticType::UndersizedRegion)
    );
    assert!(h.records().is_empty());
    h.stop().await;
}
async fn split_merge(store: Arc<dyn MetaStore>) {
    let mut h = Host::new(store);
    h.apply(region(1, "", "", 2)).await;
    h.apply(region(2, "m", "", 3)).await;
    assert!(
        h.state
            .lock()
            .unwrap()
            .cluster
            .regions
            .get_region(1)
            .is_none()
    );
    assert!(h.endpoint.load_region(&h.paths, 1).await.unwrap().is_none());
    h.apply(region(1, "", "m", 3)).await;
    {
        let r = h.cached(2);
        h.state.lock().unwrap().label_stats.observe(&r, &[], &[]);
    }
    h.apply(region(1, "", "", 4)).await;
    let s = h.state.lock().unwrap();
    assert_eq!(s.cluster.regions.get_regions().len(), 1);
    assert!(s.cluster.regions.get_region(2).is_none());
    assert!(!s.region_stats.as_ref().unwrap().is_observed(2));
    drop(s);
    assert!(h.endpoint.load_region(&h.paths, 2).await.unwrap().is_none());
    assert_eq!(
        h.endpoint.load_region(&h.paths, 1).await.unwrap(),
        Some(region(1, "", "", 4).meta().clone())
    );
    let records = h.records();
    assert_eq!(
        records
            .iter()
            .map(|r| (r.meta().id, r.meta().epoch.unwrap().version))
            .collect::<Vec<_>>(),
        vec![(1, 2), (2, 3), (1, 3), (1, 4)]
    );
    h.state.lock().unwrap().label_stats.clear_defunct_regions();
    assert_eq!(
        h.state
            .lock()
            .unwrap()
            .label_stats
            .label_counter()
            .get("none"),
        Some(&0)
    );
    assert_eq!(h.cached(1).get_ref(), 2);
    h.stop().await;
}
#[tokio::test]
async fn state_task_46_heartbeat_effects_split_merge_memory() {
    split_merge(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn state_task_46_heartbeat_effects_split_merge_etcd() {
    let fixture = effects_fixture::Fixture::start();
    split_merge(Arc::new(fixture.store().await)).await;
}
#[tokio::test]
async fn state_task_46_heartbeat_effects_etcd_disconnect_keeps_cache_and_history() {
    let mut fixture = effects_fixture::Fixture::start();
    let mut h = Host::new(Arc::new(fixture.store().await));
    h.apply(region(1, "", "", 2)).await;
    fixture.command("stop", "stopped");
    let mut receipt = h.submit(region(1, "", "", 3));
    assert_eq!(h.cached(1).meta().epoch.unwrap().version, 3);
    let failures = tokio::time::timeout(std::time::Duration::from_secs(10), receipt.wait())
        .await
        .unwrap();
    assert!(
        matches!(
            failures.as_slice(),
            [HeartbeatEffectFailure::SaveRegion(1, _)]
        ),
        "{failures:?}"
    );
    assert_eq!(h.cached(1).get_ref(), 2);
    assert_eq!(h.records().len(), 2);
    fixture.command("start", "restarted");
    assert_eq!(
        h.endpoint
            .load_region(&h.paths, 1)
            .await
            .unwrap()
            .unwrap()
            .epoch
            .unwrap()
            .version,
        2
    );
    // A duplicate does not retry a failed write, matching warm-up-only persistence.
    h.apply(region(1, "", "", 3)).await;
    assert_eq!(
        h.endpoint
            .load_region(&h.paths, 1)
            .await
            .unwrap()
            .unwrap()
            .epoch
            .unwrap()
            .version,
        2
    );
    assert_eq!(h.records().len(), 2);
    h.stop().await;
}

// Only the storage I/O failure boundary is controlled; successful operations use MemoryStore.
struct FaultStore {
    inner: MemoryStore,
    fail_keys: Mutex<std::collections::BTreeSet<Vec<u8>>>,
}
use astersql_pd_storage_kv as kv;
#[async_trait::async_trait]
impl MetaStore for FaultStore {
    async fn read(&self, r: kv::Read) -> kv::Result<kv::Snapshot> {
        self.inner.read(r).await
    }
    async fn transact(&self, c: Vec<kv::Compare>, w: Vec<kv::Mutation>) -> kv::Result<kv::Commit> {
        if w.iter().any(|m| {
            let key = match m {
                kv::Mutation::Put { key, .. } | kv::Mutation::Delete { key } => key,
            };
            self.fail_keys.lock().unwrap().contains(key)
        }) {
            return Err(kv::StoreError::Unavailable);
        }
        self.inner.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<kv::Compare>,
        t: Vec<kv::Mutation>,
        e: Vec<kv::Mutation>,
    ) -> kv::Result<kv::Commit> {
        self.inner.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<kv::Compare>,
        t: Vec<kv::RawTxnOp>,
        e: Vec<kv::RawTxnOp>,
    ) -> kv::Result<kv::RawTxnResponse> {
        self.inner.transact_raw(c, t, e).await
    }
    async fn watch(&self, p: Vec<u8>, r: kv::Revision) -> kv::Result<Box<dyn kv::MetaWatch>> {
        self.inner.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> kv::Result<kv::Lease> {
        self.inner.grant(t).await
    }
    async fn keep_alive(&self, l: kv::LeaseId) -> kv::Result<kv::Lease> {
        self.inner.keep_alive(l).await
    }
    async fn revoke(&self, l: kv::LeaseId) -> kv::Result<()> {
        self.inner.revoke(l).await
    }
    async fn ttl(&self, l: kv::LeaseId) -> kv::Result<i64> {
        self.inner.ttl(l).await
    }
    async fn compact(&self, r: kv::Revision) -> kv::Result<()> {
        self.inner.compact(r).await
    }
}
#[tokio::test]
async fn state_task_46_heartbeat_effects_delete_failure_continues_then_save_failure() {
    let store = Arc::new(FaultStore {
        inner: MemoryStore::default(),
        fail_keys: Mutex::new(Default::default()),
    });
    let mut h = Host::new(store.clone());
    h.apply(region(2, "", "m", 2)).await;
    h.apply(region(3, "m", "", 2)).await;
    store
        .fail_keys
        .lock()
        .unwrap()
        .insert(h.paths.region_path(2).into_bytes());
    let mut merged = h.submit(region(1, "", "", 3));
    let failures = merged.wait().await;
    assert!(
        matches!(
            failures.as_slice(),
            [HeartbeatEffectFailure::DeleteRegion(2, _)]
        ),
        "{failures:?}"
    );
    assert!(h.endpoint.load_region(&h.paths, 2).await.unwrap().is_some());
    assert!(h.endpoint.load_region(&h.paths, 3).await.unwrap().is_none());
    assert_eq!(
        h.endpoint
            .load_region(&h.paths, 1)
            .await
            .unwrap()
            .unwrap()
            .epoch
            .unwrap()
            .version,
        3
    );
    assert!(
        h.state
            .lock()
            .unwrap()
            .cluster
            .regions
            .get_region(2)
            .is_none()
    );
    store
        .fail_keys
        .lock()
        .unwrap()
        .insert(h.paths.region_path(1).into_bytes());
    let mut update = h.submit(region(1, "", "", 4));
    assert!(matches!(
        update.wait().await.as_slice(),
        [HeartbeatEffectFailure::SaveRegion(1, _)]
    ));
    assert_eq!(h.cached(1).meta().epoch.unwrap().version, 4);
    assert_eq!(
        h.endpoint
            .load_region(&h.paths, 1)
            .await
            .unwrap()
            .unwrap()
            .epoch
            .unwrap()
            .version,
        3
    );
    assert_eq!(h.records().last().unwrap().meta().epoch.unwrap().version, 4);
    h.stop().await;
}

#[tokio::test]
async fn state_task_46_heartbeat_effects_queue_cancel_and_duplicate_repair() {
    use astersql_pd_ratelimit::ConcurrencyLimiter;
    let mut h = Host::new(Arc::new(MemoryStore::default()));
    h.subtree.stop().await;
    h.misc.stop().await;
    h.sync.stop().await;
    let limiter = Arc::new(ConcurrencyLimiter::new(0));
    h.subtree = ConcurrentRunner::new(Some(limiter.clone()));
    h.misc = ConcurrentRunner::new(Some(limiter.clone()));
    h.sync = ConcurrentRunner::new(Some(limiter.clone()));
    let parent = CancellationToken::new();
    h.subtree.start(&parent);
    h.misc.start(&parent);
    h.sync.start(&parent);
    let mut receipt = h.submit(region(1, "", "", 2));
    assert_eq!(h.cached(1).get_ref(), 1);
    h.stop().await;
    let failures = receipt.wait().await;
    assert_eq!(failures.len(), 4);
    assert!(
        failures
            .iter()
            .all(|f| matches!(f, HeartbeatEffectFailure::Cancelled(_)))
    );
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
    assert_eq!(limiter.get_running_tasks_num(), 0);
    assert!(h.endpoint.load_region(&h.paths, 1).await.unwrap().is_none());
    assert!(h.records().is_empty());
    // Retained duplicate repairs only the missing subtree; it does not invent KV/history.
    h.subtree = ConcurrentRunner::new(None);
    h.misc = ConcurrentRunner::new(None);
    h.sync = ConcurrentRunner::new(None);
    h.subtree.start(&parent);
    h.misc.start(&parent);
    h.sync.start(&parent);
    h.apply(region(1, "", "", 2)).await;
    assert_eq!(h.cached(1).get_ref(), 2);
    assert!(h.records().is_empty());
    assert!(h.endpoint.load_region(&h.paths, 1).await.unwrap().is_none());
    h.stop().await;
}

#[tokio::test]
async fn state_task_46_heartbeat_effects_delayed_stats_refresh_root() {
    use astersql_pd_ratelimit::ConcurrencyLimiter;
    let mut h = Host::new(Arc::new(MemoryStore::default()));
    h.misc.stop().await;
    let limiter = Arc::new(ConcurrencyLimiter::new(1));
    let held = limiter
        .acquire_token(&CancellationToken::new())
        .await
        .unwrap();
    h.misc = ConcurrentRunner::new(Some(limiter.clone()));
    h.misc.start(&CancellationToken::new());
    let mut first = h.submit(region(1, "", "", 2));
    // Commit directly so only the old queued collection can classify this latest root.
    let mut latest = region(1, "", "", 3);
    latest.stats_mut().approximate_size = 144;
    handle_region_heartbeat(&mut h.state.lock().unwrap().cluster, latest, false).unwrap();
    limiter.release_token(&held);
    assert!(first.wait().await.is_empty());
    assert!(
        h.state
            .lock()
            .unwrap()
            .region_stats
            .as_ref()
            .unwrap()
            .is_region_stats_type(1, RegionStatisticType::OversizedRegion)
    );
    assert_eq!(h.cached(1).meta().epoch.unwrap().version, 3);
    // Persistence and history intentionally retain the submitted metadata, unlike Collect.
    assert_eq!(
        h.endpoint
            .load_region(&h.paths, 1)
            .await
            .unwrap()
            .unwrap()
            .epoch
            .unwrap()
            .version,
        2
    );
    assert_eq!(h.records()[0].meta().epoch.unwrap().version, 2);
    h.stop().await;
}

async fn aged_runner() -> (
    ConcurrentRunner,
    Arc<astersql_pd_ratelimit::ConcurrencyLimiter>,
    astersql_pd_ratelimit::TaskToken,
) {
    use astersql_pd_ratelimit::ConcurrencyLimiter;
    let limiter = Arc::new(ConcurrencyLimiter::new(1));
    let held = limiter
        .acquire_token(&CancellationToken::new())
        .await
        .unwrap();
    let mut runner = ConcurrentRunner::new(Some(limiter.clone()))
        .with_queue_limits(std::time::Duration::from_millis(1), 100);
    runner.start(&CancellationToken::new());
    runner.run_task(|_| async {}).unwrap();
    while limiter.get_waiting_tasks_num() == 0 {
        tokio::task::yield_now().await;
    }
    runner.run_task(|_| async {}).unwrap(); // channel slot
    runner.run_task(|_| async {}).unwrap(); // FIFO entry that will age
    assert_eq!(runner.pending_task_count(), 1);
    tokio::time::advance(std::time::Duration::from_millis(2)).await;
    (runner, limiter, held)
}
#[tokio::test(start_paused = true)]
async fn state_task_46_heartbeat_effects_retained_admission_and_stopped_runner() {
    let mut h = Host::new(Arc::new(MemoryStore::default()));
    h.subtree.stop().await;
    h.sync.stop().await;
    let (subtree, sub_limit, sub_held) = aged_runner().await;
    let (sync, sync_limit, sync_held) = aged_runner().await;
    h.subtree = subtree;
    h.sync = sync;
    let mut new = h.submit(region(1, "", "", 2));
    sub_limit.release_token(&sub_held);
    sync_limit.release_token(&sync_held);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(10), new.wait())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(h.cached(1).get_ref(), 2);
    assert_eq!(h.records().len(), 1);
    // A size-only update is not retained and must honor pending age rejection.
    h.subtree.stop().await;
    let (subtree, age_limit, age_held) = aged_runner().await;
    h.subtree = subtree;
    let mut size = region(1, "", "", 2);
    size.stats_mut().approximate_size = 144;
    let mut update = h.submit(size);
    let errors = update.wait().await;
    assert!(
        matches!(
            errors.as_slice(),
            [HeartbeatEffectFailure::Admission(
                HeartbeatEffect::UpdateSubTree,
                astersql_pd_ratelimit::RunnerError::MaxWaitingTasksExceeded
            )]
        ),
        "{errors:?}"
    );
    assert_eq!(h.cached(1).stats().approximate_size, 144);
    assert_eq!(h.records().len(), 1);
    // Rejection is observable but root admission remains successful after stop, too.
    h.stop().await;
    age_limit.release_token(&age_held);
    let mut stopped = h.submit(region(1, "", "", 3));
    let errors = stopped.wait().await;
    assert_eq!(errors.len(), 4);
    assert!(errors.iter().all(|e| matches!(
        e,
        HeartbeatEffectFailure::Admission(_, astersql_pd_ratelimit::RunnerError::Stopped)
    )));
    assert_eq!(h.cached(1).meta().epoch.unwrap().version, 3);
    h.stop().await;
}

#[tokio::test]
async fn state_task_46_heartbeat_effects_bucket_peer_epoch_and_stale() {
    let store = Arc::new(MemoryStore::default());
    let mut h = Host::new(store.clone());
    let r = region(1, "", "", 2);
    h.apply(r.clone()).await;
    let revision = store
        .read(kv::Read::prefix(b"", kv::Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    let mut bucket = r.clone();
    bucket.set_bucket_meta(Some(RegionBucketMeta {
        version: 4,
        keys: vec![],
    }));
    let result = h.apply(bucket.clone()).await;
    assert_eq!(
        result.guide,
        RegionGuide {
            save_kv: true,
            save_cache: true,
            ..Default::default()
        }
    );
    assert!(
        store
            .read(kv::Read::prefix(b"", kv::Consistency::Linearizable))
            .await
            .unwrap()
            .revision
            > revision
    );
    let mut meta = r.meta().clone();
    meta.peers.pop();
    let mut peers = RegionInfo::new(meta, r.leader().cloned(), vec![], vec![]);
    *peers.stats_mut() = r.stats().clone();
    peers.set_source(RegionSource::Heartbeat);
    assert!(h.apply(peers.clone()).await.guide.save_kv);
    assert_eq!(
        h.endpoint
            .load_region(&h.paths, 1)
            .await
            .unwrap()
            .unwrap()
            .peers
            .len(),
        2
    );
    let result = h.apply(region(1, "", "", 3)).await;
    assert!(result.guide.save_kv && result.guide.retained);
    let root = h.cached(1);
    let records = h.records().len();
    let revision = store
        .read(kv::Read::prefix(b"", kv::Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    let stale = submit_region_heartbeat(
        &h.state,
        r,
        false,
        &HeartbeatRunners {
            subtree: &h.subtree,
            misc: &h.misc,
            sync: &h.sync,
        },
        Some(h.endpoint.clone()),
        h.paths,
        Some(h.history.clone()),
        100,
    );
    assert!(stale.is_err());
    assert!(Arc::ptr_eq(&root, &h.cached(1)));
    assert_eq!(h.records().len(), records);
    assert_eq!(
        store
            .read(kv::Read::prefix(b"", kv::Consistency::Linearizable))
            .await
            .unwrap()
            .revision,
        revision
    );
    h.stop().await;
}

#[tokio::test]
async fn task_128_pipeline_overlap_cleanup_and_stopped_lane() {
    use astersql_pd_schedule_affinity::{
        group::Group,
        manager::Manager,
        ranges::{GroupKeyRanges, make_label_rule},
    };
    use astersql_pd_schedule_labeler::RegionLabeler;
    for stopped in [false, true] {
        let mut h = Host::new(Arc::new(MemoryStore::default()));
        h.apply(region(1, "a", "m", 2)).await;
        h.apply(region(2, "m", "z", 2)).await;
        let old = [h.cached(1), h.cached(2)];
        let mut labeler = RegionLabeler::load(
            StorageEndpoint::new(Arc::new(MemoryStore::default())),
            h.paths,
            0,
        )
        .await
        .unwrap();
        let mut rule = make_label_rule(&GroupKeyRanges {
            group_id: "g".into(),
            key_ranges: vec![astersql_pd_keyutil::KeyRange::new(
                b"a".to_vec(),
                b"z".to_vec(),
            )],
        });
        rule.check_and_adjust(0).unwrap();
        let mut affinity = Manager::default();
        affinity.sync_key_ranges_from_etcd(&rule).unwrap();
        affinity.sync_group_from_etcd(
            &Group {
                id: "g".into(),
                leader_store_id: 1,
                voter_store_ids: vec![1, 2, 3],
                ..Default::default()
            },
            &labeler,
            100,
        );
        labeler.set_label_rule(rule, 0).await.unwrap();
        affinity.restore_affinity_group("g", 100);
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.enable_placement_rules_cache = true;
        let mut rules = RuleManager::new(h.endpoint.clone(), h.paths);
        rules
            .initialize(3, &[], "", false, &conf, &[])
            .await
            .unwrap();
        let stores: Vec<_> = (1..=3)
            .map(|id| {
                Arc::new(StoreInfo::new(StoreMeta {
                    id,
                    last_heartbeat: 1,
                    ..Default::default()
                }))
            })
            .collect();
        {
            let mut state = h.state.lock().unwrap();
            for r in &old {
                assert!(
                    state
                        .region_stats
                        .as_ref()
                        .unwrap()
                        .is_observed(r.meta().id)
                );
                state.label_stats.observe(r, &[], &[]);
                assert!(
                    affinity
                        .get_and_cache_region_affinity_group_state(&labeler, Some(r), 100)
                        .1
                );
                let fit = rules.fit_region(&stores, r, &conf, 1);
                rules.set_region_fit_cache(&stores, r, fit, 1);
                for _ in 0..11 {
                    rules.fit_region(&stores, r, &conf, 1);
                }
                assert!(rules.is_region_fit_cached(&stores, r, 1));
            }
            state.rules = rules;
            state.affinity = Some(affinity);
        }
        if stopped {
            h.misc.stop().await;
        }
        let mut receipt = h.submit(region(3, "a", "z", 3));
        assert_eq!(receipt.heartbeat.overlaps.len(), 2);
        let failures = receipt.wait().await;
        if stopped {
            assert!(failures.iter().any(|f| matches!(
                f,
                HeartbeatEffectFailure::Admission(HeartbeatEffect::HandleOverlaps, _)
            )));
        } else {
            assert!(failures.is_empty(), "{failures:?}");
        }
        {
            let mut state = h.state.lock().unwrap();
            assert!(state.cluster.regions.get_region(3).is_some());
            assert!(state.cluster.regions.get_region(1).is_none());
            assert_eq!(state.label_stats.label_counter()["none"], 2);
            state.label_stats.clear_defunct_regions();
            assert_eq!(
                state.label_stats.label_counter()["none"],
                if stopped { 2 } else { 0 }
            );
            let affinity = state.affinity.as_ref().unwrap();
            assert_eq!(
                affinity.affinity_region_count(),
                if stopped { 2 } else { 0 }
            );
            assert_eq!(
                affinity.groups["g"].info.region_count,
                if stopped { 2 } else { 0 }
            );
            for r in &old {
                assert_eq!(
                    state
                        .region_stats
                        .as_ref()
                        .unwrap()
                        .is_observed(r.meta().id),
                    stopped
                );
                assert_eq!(state.rules.is_region_fit_cached(&stores, r, 1), stopped);
            }
        }
        if !stopped {
            h.apply(region(3, "a", "z", 3)).await;
            assert_eq!(
                h.state
                    .lock()
                    .unwrap()
                    .affinity
                    .as_ref()
                    .unwrap()
                    .affinity_region_count(),
                0
            );
        }
        h.stop().await;
    }
}
