// Copyright 2026 AsterSQL.
use crate::{BasicCluster, IdAllocation, PrepareRegionCount, SharedCluster, SharedClusterView};
use astersql_pd_core::{BasicCluster as CoreCluster, KeyRange, RegionInfo, RegionMeta};
use astersql_pd_id::{Allocator, AllocatorError, AllocatorNamespace};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyutil::KeyRange as ScheduleRange;
use astersql_pd_member::{Member, MemberError};
use astersql_pd_schedule_affinity::{manager::Manager as AffinityManager, ranges::GroupKeyRanges};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_keyrange::Manager as KeyRangeManager;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::StatisticsSnapshot;
use astersql_pd_storage_domain::StorageEndpoint;
use astersql_pd_storage_domain::{MemoryStore, MetaStore};
use serde_json::{Value, json};
use std::{future::Future, sync::Arc};
use tokio_util::sync::CancellationToken;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_003.json"
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
    stats: StatisticsSnapshot,
    allocator: Allocator,
    store: Arc<dyn MetaStore>,
    paths: ClusterPaths,
}
impl State {
    async fn new() -> Self {
        let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
        let paths = ClusterPaths::new(3003);
        let config = ProviderSnapshot::default();
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
        let member = Member::bootstrap(store.clone(), cancel.clone(), 3003, 1, b"task003".to_vec())
            .await
            .unwrap();
        let leader = Arc::new(member.campaign(30).await.unwrap());
        let allocator = Allocator::with_namespace(
            store.clone(),
            leader,
            cancel,
            1,
            AllocatorNamespace::Keyspace,
        )
        .unwrap();
        Self {
            core: CoreCluster::new(),
            config,
            rules,
            labels,
            affinity,
            ranges: KeyRangeManager::new(),
            stats: StatisticsSnapshot::default(),
            allocator,
            store,
            paths,
        }
    }
    fn view<'a, P: PrepareRegionCount + ?Sized>(
        &'a self,
        prepare: &'a P,
    ) -> SharedClusterView<'a, ProviderSnapshot<String>, P> {
        SharedClusterView {
            basic_cluster: &self.core,
            shared_config: &self.config,
            rule_manager: &self.rules,
            region_labeler: &self.labels,
            affinity_manager: &self.affinity,
            key_range_manager: &self.ranges,
            statistics: &self.stats,
            id_allocation: IdAllocation::Counted(&self.allocator),
            prepare_region_count: prepare,
            recovery_running: false,
        }
    }
}

#[tokio::test]
async fn task_003_behavior_cases() {
    let mut state = State::new().await;
    let f = fixture();
    let expected = &f["outputs"];
    let view = state.view(&state.core);
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    assert_send_sync(&view);
    assert!(std::ptr::eq(view.get_basic_cluster(), &state.core));
    assert!(std::ptr::eq(view.get_shared_config(), &state.config));
    assert!(std::ptr::eq(view.get_rule_manager(), &state.rules));
    assert!(std::ptr::eq(view.get_region_labeler(), &state.labels));
    assert!(std::ptr::eq(view.get_affinity_manager(), &state.affinity));
    assert!(std::ptr::eq(view.get_key_range_manager(), &state.ranges));
    assert!(std::ptr::eq(view.stores(), &state.core.stores));
    assert!(std::ptr::eq(view.regions(), &state.core.regions));
    let rule = view.get_rule_manager().get_rule("pd", "default").unwrap();
    assert_eq!(
        json!({"group":rule.group_id,"id":rule.id,"role":rule.role,"count":rule.count}),
        expected["placement_default"]
    );
    assert!(view.get_rule_manager().get_rule("pd", "missing").is_none());
    assert!(
        view.get_region_labeler()
            .get_label_rule("missing", 0)
            .is_none()
    );
    assert!(
        view.get_affinity_manager()
            .group_observation("missing", 0)
            .is_none()
    );
    assert_eq!(
        json!(view.get_key_range_manager().is_empty()),
        expected["keyrange_empty"]
    );

    let now = f["inputs"]["now"].as_u64().unwrap();
    let range = ScheduleRange::new(b"a".to_vec(), b"z".to_vec());
    state
        .affinity
        .create_affinity_groups(
            &StorageEndpoint::new(state.store.clone()),
            &state.paths,
            &mut state.labels,
            &[GroupKeyRanges {
                group_id: "g1".into(),
                key_ranges: vec![range.clone()],
            }],
            now,
        )
        .await
        .unwrap();
    let view = state.view(&state.core);
    let observed = view
        .get_affinity_manager()
        .group_observation("g1", now)
        .unwrap();
    let group = observed.state;
    assert_eq!(
        json!({"id":group.group.id,"phase":group.phase,"range_count":group.range_count,
        "region_count":group.region_count,"affinity_region_count":group.affinity_region_count,
        "regular_scheduling_allowed":group.regular_scheduling_allowed,
        "affinity_scheduling_allowed":group.affinity_scheduling_allowed,"version":observed.affinity_version}),
        expected["affinity"]
    );
    assert_eq!(group.group.create_timestamp, now);
    let labels = view.get_region_labeler().get_all_label_rules(now as i128);
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].id, expected["label"]["id"]);
    assert_eq!(labels[0].labels[0].key, expected["label"]["key"]);
    assert_eq!(labels[0].labels[0].value, expected["label"]["value"]);
    let region = RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"z".to_vec(),
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    assert_eq!(
        view.get_region_labeler()
            .get_region_label(&region, "affinity_group", now as i128),
        expected["label"]["value"]
    );
    // Mutate the canonical range owner while both consumer views remain borrowed.
    let checker = state.view(&state.core);
    view.get_key_range_manager()
        .append(std::slice::from_ref(&range));
    assert_eq!(
        json!(checker.get_key_range_manager().is_empty()),
        expected["keyrange_populated"]
    );
    assert_eq!(
        json!(
            checker
                .get_key_range_manager()
                .get_non_overlapping_key_ranges(&range)
        ),
        expected["keyrange_covered"]
    );
    println!(
        "task_003_behavior_cases: 9 oracle scenarios passed; canonical owner identity checked"
    );
}

// Frozen external HTTP failure is injected only at the host I/O boundary.
// The shared view must return the same error object, without fabricating count=0 success.
#[derive(Debug)]
struct RemoteError {
    operation: String,
    url: String,
    cause: String,
}
impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {:?}: {}", self.operation, self.url, self.cause)
    }
}
impl std::error::Error for RemoteError {}
struct RemotePrepare(Arc<RemoteError>);
impl PrepareRegionCount for RemotePrepare {
    type Error = Arc<RemoteError>;
    fn get_prepare_region_count(&self) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        std::future::ready(Err(self.0.clone()))
    }
}

#[tokio::test]
async fn task_003_halt_and_prepare_error() {
    let mut state = State::new().await;
    let f = fixture();
    for (index, halted) in [false, true, false].into_iter().enumerate() {
        state.config.schedule.halt_scheduling = halted;
        assert_eq!(
            json!(state.view(&state.core).is_scheduling_halted()),
            f["outputs"]["halt"][index]
        );
    }
    // Source contract: classic unsafe recovery also halts scheduling.
    let mut view = state.view(&state.core);
    view.recovery_running = true;
    assert!(view.is_scheduling_halted());
    assert_eq!(
        view.get_prepare_region_count().await.unwrap(),
        view.total_region_count()
    );
    let e = &f["outputs"]["prepare_error"];
    let boundary = RemotePrepare(Arc::new(RemoteError {
        operation: e["operation"].as_str().unwrap().into(),
        url: e["url"].as_str().unwrap().into(),
        cause: e["cause"].as_str().unwrap().into(),
    }));
    let error = state
        .view(&boundary)
        .get_prepare_region_count()
        .await
        .unwrap_err();
    assert!(Arc::ptr_eq(&error, &boundary.0));
    assert_eq!(
        json!({"operation":error.operation,"url":error.url,"cause":error.cause}),
        *e
    );
    println!(
        "task_003_halt_and_prepare_error: 4 oracle observations; recovery/local count source contracts passed"
    );
}

#[tokio::test]
async fn task_003_id_allocation_success_and_failure() {
    let state = State::new().await;
    let f = fixture();
    state
        .allocator
        .set_base(f["inputs"]["allocator_base"].as_u64().unwrap())
        .await
        .unwrap();
    let view = state.view(&state.core);
    let mut actual = Vec::new();
    for count in [0, 2, 1] {
        actual.push(view.alloc_id(count).await.unwrap());
    }
    assert_eq!(json!(actual), f["outputs"]["allocation"]);
    assert!(matches!(
        view.alloc_id(1).await,
        Err(AllocatorError::Exhausted)
    ));
    // Failure is not converted to a successful zero allocation, and does not wrap/reuse IDs.
    assert_eq!(view.alloc_id(0).await.unwrap(), (16777215, 0));
    println!(
        "task_003_id_allocation_success_and_failure: 4 oracle scenarios passed; post-error base retained"
    );
}

#[tokio::test]
async fn task_003_classic_count_and_leadership_fault() {
    let state = State::new().await;
    let mut view = state.view(&state.core);
    view.id_allocation = IdAllocation::Classic(&state.allocator);
    // Classic AllocID ignores requested count, including zero; uses real persisted allocator.
    assert_eq!(view.alloc_id(0).await.unwrap(), (1, 1));
    assert_eq!(view.alloc_id(5).await.unwrap(), (2, 1));
    state.store.revoke(1).await.unwrap();
    assert!(matches!(
        view.alloc_id(5).await,
        Err(AllocatorError::Member(MemberError::NotLeader))
    ));
    println!(
        "task_003_classic_count_and_leadership_fault: 3 source-contract/fault scenarios passed"
    );
}

#[tokio::test]
async fn task_003_local_prepare_count_uses_canonical_regions() {
    let mut state = State::new().await;
    // Count/ranges from existing newTestRegions(3,3,3); payload fields not read
    // by this contract are covered by the preceding basic-cluster task.
    for i in 1..=3 {
        assert!(
            state
                .core
                .regions
                .check_and_put_region(Arc::new(RegionInfo::new(
                    RegionMeta {
                        id: i,
                        key_range: KeyRange {
                            start_key: format!("a{:20}", i + 1).into_bytes(),
                            end_key: format!("a{:20}", i + 2).into_bytes(),
                        },
                        ..Default::default()
                    },
                    None,
                    vec![],
                    vec![],
                )))
                .is_empty()
        );
    }
    let view = state.view(&state.core);
    assert_eq!(
        json!(view.get_prepare_region_count().await.unwrap()),
        fixture()["outputs"]["local_prepare_count"]
    );
    assert_eq!(view.total_region_count(), 3);
}

struct RemoteAllocation(Arc<String>);
impl crate::SharedIdAllocator for RemoteAllocation {
    type Error = Arc<String>;
    fn alloc_id(
        &self,
        _count: u32,
    ) -> impl Future<Output = Result<(u64, u32), Self::Error>> + Send {
        // Captured Go missing-leader response at the external RPC boundary.
        std::future::ready(Err(self.0.clone()))
    }
}
#[tokio::test]
async fn task_003_remote_allocation_error_is_preserved() {
    let state = State::new().await;
    let f = fixture();
    let response = Arc::new(
        f["outputs"]["remote_allocation_error"]
            .as_str()
            .unwrap()
            .to_owned(),
    );
    let local = state.view(&state.core);
    let view = SharedClusterView {
        basic_cluster: local.basic_cluster,
        shared_config: local.shared_config,
        region_labeler: local.region_labeler,
        rule_manager: local.rule_manager,
        key_range_manager: local.key_range_manager,
        affinity_manager: local.affinity_manager,
        statistics: local.statistics,
        id_allocation: RemoteAllocation(response.clone()),
        prepare_region_count: local.prepare_region_count,
        recovery_running: false,
    };
    let actual = view.alloc_id(3).await.unwrap_err();
    assert!(Arc::ptr_eq(&actual, &response));
    assert_eq!(*actual, f["outputs"]["remote_allocation_error"]);
}
