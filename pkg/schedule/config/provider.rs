// Copyright 2026 AsterSQL.
//! Host-owned configuration views. A view borrows one complete immutable snapshot.
use crate::{MergeAgainError, ReplicationConfig, ScheduleConfig, StoreConfig, StoreLimitConfig};
use astersql_pd_core::StoreLabel;
use astersql_pd_core_constant::{KeyType, SchedulePolicy, string_to_schedule_policy};
use astersql_pd_schedule_types::{CheckerSchedulerType, scheduler_type_compatible};
use astersql_pd_storelimit::Type;
use astersql_pd_typeutil::Duration;
use std::collections::{BTreeMap, BTreeSet};

/// Label property preventing a store from receiving leaders.
pub const REJECT_LEADER: &str = "reject-leader";
/// Registration contains types only; no scheduler factories or process global state.
#[derive(Clone, Debug, Default)]
pub struct SchedulerRegistry {
    types: BTreeSet<CheckerSchedulerType>,
}
impl SchedulerRegistry {
    /// Registers a type idempotently, including custom types.
    pub fn register(&mut self, typ: CheckerSchedulerType) {
        self.types.insert(typ);
    }
    /// Tests exact type identity.
    pub fn contains(&self, typ: &CheckerSchedulerType) -> bool {
        self.types.contains(typ)
    }
    /// Iterates deterministically without implying scheduler implementations exist.
    pub fn registered(&self) -> impl Iterator<Item = &CheckerSchedulerType> {
        self.types.iter()
    }
}
/// One host-captured generation. Version is the host's domain version type; it
/// is generic to avoid depending on server/config or duplicating its version model.
#[derive(Clone, Debug)]
pub struct ProviderSnapshot<V> {
    pub schedule: ScheduleConfig,
    pub replication: ReplicationConfig,
    pub store: StoreConfig,
    pub label_properties: BTreeMap<String, Vec<StoreLabel>>,
    pub cluster_version: V,
    pub key_type: KeyType,
    pub flow_round_by_digit: i64,
}
impl<V: Default> Default for ProviderSnapshot<V> {
    fn default() -> Self {
        Self {
            schedule: ScheduleConfig::default(),
            replication: ReplicationConfig::default(),
            store: StoreConfig::default(),
            label_properties: BTreeMap::new(),
            cluster_version: V::default(),
            key_type: KeyType::TABLE,
            flow_round_by_digit: 0,
        }
    }
}
/// Shared read contract. All getters use the same borrowed generation.
pub trait SharedConfigProvider {
    type Version;
    fn snapshot(&self) -> &ProviderSnapshot<Self::Version>;
    fn get_cluster_version(&self) -> &Self::Version {
        &self.snapshot().cluster_version
    }
    fn get_key_type(&self) -> KeyType {
        self.snapshot().key_type
    }
    fn get_location_labels(&self) -> &[String] {
        self.snapshot().replication.location_labels.as_slice()
    }
    fn check_label_property(&self, typ: &str, labels: &[StoreLabel]) -> bool {
        self.snapshot()
            .label_properties
            .get(typ)
            .is_some_and(|rules| {
                rules
                    .iter()
                    .any(|rule| labels.iter().any(|label| label == rule))
            })
    }
    fn get_store_limit_by_type(&self, id: u64, typ: Type) -> f64 {
        let cfg = &self.snapshot().schedule;
        let limit = cfg
            .store_limit
            .as_ref()
            .and_then(|limits| limits.get(&id))
            .unwrap_or(&cfg.default_store_limit);
        match typ {
            Type::ADD_PEER => limit.add_peer,
            Type::REMOVE_PEER => limit.remove_peer,
            Type::TRANSFER_LEADER_IN => limit.transfer_leader_in,
            Type::SEND_SNAPSHOT => 0.0,
            _ => panic!("no such limit type"),
        }
    }
    fn get_max_replicas(&self) -> i64 {
        self.snapshot().replication.max_replicas as i64
    }
    fn is_placement_rules_enabled(&self) -> bool {
        self.snapshot().replication.enable_placement_rules
    }
    fn is_placement_rules_cache_enabled(&self) -> bool {
        self.snapshot().replication.enable_placement_rules_cache
    }
    fn get_affinity_schedule_limit(&self) -> u64 {
        self.snapshot().schedule.affinity_schedule_limit
    }
    fn get_split_scatter_schedule_limit(&self) -> u64 {
        self.snapshot().schedule.split_scatter_schedule_limit
    }
    fn get_max_snapshot_count(&self) -> u64 {
        self.snapshot().schedule.max_snapshot_count
    }
    fn get_max_pending_peer_count(&self) -> u64 {
        self.snapshot().schedule.max_pending_peer_count
    }
    fn get_low_space_ratio(&self) -> f64 {
        self.snapshot().schedule.low_space_ratio
    }
    fn get_high_space_ratio(&self) -> f64 {
        self.snapshot().schedule.high_space_ratio
    }
    fn get_max_store_down_time(&self) -> Duration {
        self.snapshot().schedule.max_store_down_time
    }
    fn is_use_joint_consensus(&self) -> bool {
        self.snapshot().schedule.enable_joint_consensus
    }
    fn is_cross_table_merge_enabled(&self) -> bool {
        self.snapshot().schedule.enable_cross_table_merge
    }
    fn is_one_way_merge_enabled(&self) -> bool {
        self.snapshot().schedule.enable_one_way_merge
    }
    fn get_merge_schedule_limit(&self) -> u64 {
        self.snapshot().schedule.merge_schedule_limit
    }
    fn get_region_score_formula_version(&self) -> &str {
        &self.snapshot().schedule.region_score_formula_version
    }
    fn get_scheduler_max_waiting_operator(&self) -> u64 {
        self.snapshot().schedule.scheduler_max_waiting_operator
    }
    fn is_witness_allowed(&self) -> bool {
        self.snapshot().schedule.enable_witness
    }
    fn get_hot_region_cache_hits_threshold(&self) -> i64 {
        self.snapshot().schedule.hot_region_cache_hits_threshold as i64
    }
    fn get_region_schedule_limit(&self) -> u64 {
        self.snapshot().schedule.region_schedule_limit
    }
    fn get_max_affinity_merge_region_size(&self) -> u64 {
        self.snapshot().schedule.max_affinity_merge_region_size
    }
}
/// Scheduler view, with scheduler identity translated by the canonical type map.
pub trait SchedulerConfigProvider: SharedConfigProvider {
    fn is_scheduler_disabled(&self, typ: &CheckerSchedulerType) -> bool {
        let old = scheduler_type_compatible(typ).unwrap_or("");
        self.snapshot()
            .schedule
            .schedulers
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|s| s.r#type == old)
            .is_some_and(|s| s.disable)
    }
    fn get_schedule_config(&self) -> &ScheduleConfig {
        &self.snapshot().schedule
    }
    fn get_stores_limit(&self) -> Option<&BTreeMap<u64, StoreLimitConfig>> {
        self.snapshot().schedule.store_limit.as_ref()
    }
    fn get_leader_schedule_policy(&self) -> SchedulePolicy {
        string_to_schedule_policy(&self.snapshot().schedule.leader_schedule_policy)
    }
    fn is_trace_region_flow(&self) -> bool {
        self.snapshot().flow_round_by_digit <= 5
    }
    fn get_leader_schedule_limit(&self) -> u64 {
        self.snapshot().schedule.leader_schedule_limit
    }
    fn get_hot_region_schedule_limit(&self) -> u64 {
        self.snapshot().schedule.hot_region_schedule_limit
    }
    fn get_witness_schedule_limit(&self) -> u64 {
        self.snapshot().schedule.witness_schedule_limit
    }
    fn get_max_movable_hot_peer_size(&self) -> i64 {
        self.snapshot().schedule.max_movable_hot_peer_size
    }
    fn get_tolerant_size_ratio(&self) -> f64 {
        self.snapshot().schedule.tolerant_size_ratio
    }
    fn is_debug_metrics_enabled(&self) -> bool {
        self.snapshot().schedule.enable_debug_metrics
    }
    fn is_diagnostic_allowed(&self) -> bool {
        self.snapshot().schedule.enable_diagnostic
    }
    fn get_slow_store_evicting_affected_store_ratio_threshold(&self) -> f64 {
        self.snapshot()
            .schedule
            .slow_store_evicting_affected_store_ratio_threshold
    }
}
/// Store value and real merge-threshold checks, independent of transport.
pub trait StoreConfigProvider {
    fn store_config(&self) -> &StoreConfig;
    fn get_region_max_size(&self) -> u64 {
        self.store_config().get_region_max_size()
    }
    fn get_region_max_keys(&self) -> u64 {
        self.store_config().get_region_max_keys()
    }
    fn get_region_split_size(&self) -> u64 {
        self.store_config().get_region_split_size()
    }
    fn get_region_split_keys(&self) -> u64 {
        self.store_config().get_region_split_keys()
    }
    fn is_enable_region_bucket(&self) -> bool {
        self.store_config().is_enable_region_bucket()
    }
    fn is_raft_kv2(&self) -> bool {
        self.store_config().is_raft_kv2()
    }
    fn get_unified_read_pool_max_thread_count(&self) -> u64 {
        self.store_config().get_unified_read_pool_max_thread_count()
    }
    fn check_region_size(&self, size: u64, merge: u64) -> Result<(), MergeAgainError> {
        self.store_config().check_region_size(size, merge)
    }
    fn check_region_keys(&self, keys: u64, merge: u64) -> Result<(), MergeAgainError> {
        self.store_config().check_region_keys(keys, merge)
    }
}
/// Checker view shares the same schedule, replication and store generation.
pub trait CheckerConfigProvider: SharedConfigProvider + StoreConfigProvider {
    fn get_switch_witness_interval(&self) -> Duration {
        self.snapshot().schedule.switch_witness_interval
    }
    fn is_remove_extra_replica_enabled(&self) -> bool {
        self.snapshot().schedule.enable_remove_extra_replica
    }
    fn is_remove_down_replica_enabled(&self) -> bool {
        self.snapshot().schedule.enable_remove_down_replica
    }
    fn is_replace_offline_replica_enabled(&self) -> bool {
        self.snapshot().schedule.enable_replace_offline_replica
    }
    fn is_make_up_replica_enabled(&self) -> bool {
        self.snapshot().schedule.enable_make_up_replica
    }
    fn is_location_replacement_enabled(&self) -> bool {
        self.snapshot().schedule.enable_location_replacement
    }
    fn get_split_merge_interval(&self) -> Duration {
        self.snapshot().schedule.split_merge_interval
    }
    fn get_patrol_region_interval(&self) -> Duration {
        self.snapshot().schedule.patrol_region_interval
    }
    fn get_patrol_region_worker_count(&self) -> i64 {
        self.snapshot().schedule.patrol_region_worker_count
    }
    fn get_max_merge_region_size(&self) -> u64 {
        self.snapshot().schedule.max_merge_region_size
    }
    fn get_max_merge_region_keys(&self) -> u64 {
        let config = &self.snapshot().schedule;
        if config.max_merge_region_keys != 0 {
            config.max_merge_region_keys
        } else {
            config.max_merge_region_size.wrapping_mul(10_000)
        }
    }
    fn get_replica_schedule_limit(&self) -> u64 {
        self.snapshot().schedule.replica_schedule_limit
    }
    fn get_isolation_level(&self) -> &str {
        &self.snapshot().replication.isolation_level
    }
}
/// Combined read contract. Mutation/publication and persistence are host orchestration.
pub trait ConfProvider: SchedulerConfigProvider + CheckerConfigProvider {}
impl<V> SharedConfigProvider for ProviderSnapshot<V> {
    type Version = V;
    fn snapshot(&self) -> &Self {
        self
    }
}
impl<V> SchedulerConfigProvider for ProviderSnapshot<V> {}
impl<V> StoreConfigProvider for ProviderSnapshot<V> {
    fn store_config(&self) -> &StoreConfig {
        &self.store
    }
}
impl<V> CheckerConfigProvider for ProviderSnapshot<V> {}
impl<V> ConfProvider for ProviderSnapshot<V> {}
