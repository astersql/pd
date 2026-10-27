// Copyright 2026 AsterSQL.
use crate::{
    BasicCluster, BucketQueryError, CheckerCluster, ClusterViews, IdAllocation, SchedulerCluster,
    SharedCluster, SharedClusterView, StoreConfigCluster,
};
use astersql_pd_core::{
    BasicCluster as CoreCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreLabel,
    StoreMeta,
};
use astersql_pd_id::Allocator;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_member::Member;
use astersql_pd_schedule_affinity::manager::Manager as AffinityManager;
use astersql_pd_schedule_config::{
    CheckerConfigProvider, MergeAgainError, ProviderSnapshot, SchedulerConfigProvider,
    SharedConfigProvider, StoreConfigProvider,
};
use astersql_pd_schedule_keyrange::Manager as KeyRangeManager;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{LabelStatistics, StatisticsSnapshot, StoreStatInformer, StoresStats};
use astersql_pd_statistics_buckets::{
    BucketFlows, BucketRangeIndex, BucketReport, BucketStats, BucketSubmitError, BucketWaitError,
    BucketWorker, BucketWorkerClient,
};
use astersql_pd_storage_domain::StorageEndpoint;
use astersql_pd_storage_domain::{MemoryStore, MetaStore};
use prometheus::Registry;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_004.json"
    ))
    .unwrap()
}
struct State {
    core: CoreCluster,
    config: ProviderSnapshot<String>,
    rules: RuleManager,
    labels: RegionLabeler,
    affinity: AffinityManager,
    ranges: KeyRangeManager,
    region_stats: StatisticsSnapshot,
    allocator: Allocator,
    store_stats: Arc<StoresStats>,
    label_stats: LabelStatistics,
}
impl State {
    async fn new() -> Self {
        let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
        let paths = ClusterPaths::new(4004);
        let mut config = ProviderSnapshot::default();
        // The Go host adjusts defaults before publishing this generation.
        config.replication.max_replicas = 3;
        config.schedule.max_merge_region_size = 54;
        config.schedule.enable_diagnostic = true;
        config.replication.location_labels = vec!["zone".into()].into();
        let mut rules =
            RuleManager::new(Arc::new(StorageEndpoint::new(store.clone())), paths.clone());
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
            .await
            .unwrap();
        let affinity =
            AffinityManager::initialize(&StorageEndpoint::new(store.clone()), &paths, &labels, 0)
                .await
                .unwrap();
        let cancel = CancellationToken::new();
        let member = Member::bootstrap(store.clone(), cancel.clone(), 4004, 1, b"task004".to_vec())
            .await
            .unwrap();
        let leader = Arc::new(member.campaign(30).await.unwrap());
        let allocator = Allocator::new(store, leader, cancel, 1).unwrap();
        Self {
            core: CoreCluster::new(),
            config,
            rules,
            labels,
            affinity,
            ranges: KeyRangeManager::new(),
            region_stats: StatisticsSnapshot::default(),
            allocator,
            store_stats: Arc::new(StoresStats::new()),
            label_stats: LabelStatistics::new(),
        }
    }
    fn view<'a>(
        &'a mut self,
        buckets: &'a BucketWorkerClient,
    ) -> ClusterViews<'a, SharedClusterView<'a, ProviderSnapshot<String>, CoreCluster>> {
        ClusterViews {
            shared: SharedClusterView {
                basic_cluster: &self.core,
                shared_config: &self.config,
                region_labeler: &self.labels,
                rule_manager: &self.rules,
                key_range_manager: &self.ranges,
                affinity_manager: &self.affinity,
                statistics: &self.region_stats,
                id_allocation: IdAllocation::Classic(&self.allocator),
                prepare_region_count: &self.core,
                recovery_running: false,
            },
            store_statistics: self.store_stats.as_ref(),
            bucket_statistics: buckets,
            label_statistics: &mut self.label_stats,
        }
    }
}
fn report(id: u64, keys: &[&str]) -> BucketReport {
    let zeros = vec![0; keys.len() - 1];
    BucketReport {
        region_id: id,
        version: 1,
        keys: keys.iter().map(|s| s.as_bytes().to_vec()).collect(),
        period_in_ms: 1000,
        flows: BucketFlows {
            read_bytes: zeros.clone(),
            read_keys: zeros.clone(),
            read_qps: zeros.clone(),
            write_bytes: zeros.clone(),
            write_keys: zeros.clone(),
            write_qps: zeros,
        },
    }
}
fn bucket_json(stats: BucketStats) -> Value {
    let mut result = serde_json::Map::new();
    for (id, stats) in stats {
        result.insert(
            id.to_string(),
            json!(stats.iter().map(|s|json!({
            "region_id":s.region_id,"start_key":String::from_utf8(s.start_key.clone()).unwrap(),
            "end_key":String::from_utf8(s.end_key.clone()).unwrap(),"hot_degree":s.hot_degree,
            "interval":s.interval,"loads":s.loads,
        })).collect::<Vec<_>>()),
        );
    }
    Value::Object(result)
}
fn region(id: u64) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            id,
            peers: (1..=4)
                .map(|i| Peer {
                    id: i + 4,
                    store_id: i,
                    ..Default::default()
                })
                .collect(),
            key_range: KeyRange {
                start_key: vec![(id - 8) as u8],
                end_key: vec![(id - 7) as u8],
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )
}

#[tokio::test]
async fn task_004_behavior_cases() {
    let mut state = State::new().await;
    let parent = CancellationToken::new();
    let mut worker = BucketWorker::start(
        &Handle::current(),
        &parent,
        &Registry::new(),
        BucketRangeIndex::new(),
    )
    .unwrap();
    let client = worker.client();
    let f = fixture();
    let expected = &f["outputs"];
    for id in 1..=4 {
        state.store_stats.get_or_create_rolling_store_stats(id);
    }
    let store_owner = state.store_stats.clone();
    let view = state.view(&client);
    fn scheduler<C: SchedulerCluster>(c: &C) -> Value {
        json!(c.get_stores_loads())
    }
    assert_eq!(scheduler(&view), expected["stores_initial"]);
    assert!(std::ptr::eq(
        view.get_basic_cluster(),
        view.shared.get_basic_cluster()
    ));
    assert!(std::ptr::eq(view.stores(), view.shared.stores()));
    assert!(std::ptr::eq(
        view.get_scheduler_config(),
        view.get_shared_config()
    ));
    assert!(std::ptr::eq(
        view.get_checker_config(),
        view.get_shared_config()
    ));
    assert!(std::ptr::eq(
        view.get_store_config(),
        view.get_shared_config()
    ));
    // The same view sees canonical store updates while retaining detached results.
    store_owner.set_regions_stats(&[(1, 30., 40.)]);
    assert_eq!(json!(view.get_stores_loads()), expected["stores_updated"]);
    let snapshot = view.get_stores_loads();
    store_owner.remove_rolling_store_stats(1);
    assert_eq!(json!(view.get_stores_loads()), expected["stores_removed"]);
    assert_eq!(json!(snapshot), expected["stores_updated"]);
    client.check_async(report(1, &["10", "20", "30"])).unwrap();
    let cancel = CancellationToken::new();
    let first = view.buckets_stats(-20, vec![], &cancel).await.unwrap();
    assert_eq!(bucket_json(first.clone()), expected["buckets_first"]);
    assert_eq!(
        bucket_json(view.buckets_stats(0, vec![], &cancel).await.unwrap()),
        expected["buckets_degree_zero"]
    );
    client.check_async(report(2, &["20", "30"])).unwrap();
    assert_eq!(
        bucket_json(view.buckets_stats(-20, vec![2], &cancel).await.unwrap()),
        expected["buckets_selected"]
    );
    assert_eq!(
        bucket_json(view.buckets_stats(-20, vec![999], &cancel).await.unwrap()),
        expected["buckets_missing"]
    );
    assert_eq!(bucket_json(first), expected["buckets_first"]);
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn task_004_checker_label_updates() {
    let mut state = State::new().await;
    let f = fixture();
    let expected = &f["outputs"]["label_counters"];
    for (id, zone) in (1..=4).zip(["1", "2", "3", "3"]) {
        let mut labels = vec![StoreLabel {
            key: "zone".into(),
            value: zone.into(),
        }];
        if id == 4 {
            labels.push(StoreLabel {
                key: "engine".into(),
                value: "tiflash".into(),
            });
        }
        state
            .core
            .stores
            .put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                labels,
                ..Default::default()
            })));
    }
    let parent = CancellationToken::new();
    let mut worker = BucketWorker::start(
        &Handle::current(),
        &parent,
        &Registry::new(),
        BucketRangeIndex::new(),
    )
    .unwrap();
    let client = worker.client();
    let r1 = region(9);
    let r2 = region(10);
    fn update<C: CheckerCluster>(c: &mut C, regions: &[&RegionInfo]) {
        c.update_regions_label_level_stats(regions);
    }
    update(&mut state.view(&client), &[&r1]);
    assert_eq!(json!(state.label_stats.label_counter()), expected[0]);
    update(&mut state.view(&client), &[&r2]);
    assert_eq!(json!(state.label_stats.label_counter()), expected[1]);
    // Heartbeat overlap accounting belongs to the host, as in Go's controller.
    state.label_stats.mark_defunct_region(10);
    update(&mut state.view(&client), &[&r1, &r2]);
    assert_eq!(json!(state.label_stats.label_counter()), expected[2]);
    state.label_stats.mark_defunct_region(9);
    update(&mut state.view(&client), &[]);
    assert_eq!(state.label_stats.label_counter()["zone"], 0); // source boundary: empty batch drains marks
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn task_004_config_and_typed_faults() {
    let mut state = State::new().await;
    let expected = fixture();
    let expected = &expected["outputs"];
    let parent = CancellationToken::new();
    let mut worker = BucketWorker::start(
        &Handle::current(),
        &parent,
        &Registry::new(),
        BucketRangeIndex::new(),
    )
    .unwrap();
    let client = worker.client();
    for phase in 0..2 {
        if phase == 1 {
            state.config.replication.max_replicas = 5;
            state.config.schedule.max_merge_region_size = 7;
            state.config.schedule.enable_diagnostic = false;
        }
        let view = state.view(&client);
        let scheduler = view.get_scheduler_config();
        let checker = view.get_checker_config();
        let store = view.get_store_config();
        assert_eq!(scheduler.get_max_replicas(), checker.get_max_replicas());
        assert_eq!(
            json!({"diagnostic":scheduler.is_diagnostic_allowed(),"max_replicas":checker.get_max_replicas(),"max_merge_region_size":checker.get_max_merge_region_size(),"region_max_size":store.get_region_max_size()}),
            expected["configs"][phase]
        );
        for check in expected["store_checks"].as_array().unwrap() {
            let result = store.check_region_size(
                check["size"].as_u64().unwrap(),
                check["merge"].as_u64().unwrap(),
            );
            assert_eq!(
                result,
                if check["error"].is_null() {
                    Ok(())
                } else {
                    Err(MergeAgainError::Size)
                }
            );
        }
    }
    // Actual canonical worker faults; Rust's typed/cancellable boundary is an extension of Go's map return.
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert_eq!(
        state
            .view(&client)
            .buckets_stats(-20, vec![], &cancellation)
            .await
            .unwrap_err(),
        BucketQueryError::Wait(BucketWaitError::Cancelled)
    );
    worker.shutdown().await.unwrap();
    assert_eq!(
        state
            .view(&client)
            .buckets_stats(-20, vec![], &CancellationToken::new())
            .await
            .unwrap_err(),
        BucketQueryError::Submit(BucketSubmitError::Closed)
    );
}
