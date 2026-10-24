// Copyright 2026 AsterSQL.
//! Scheduling configuration values; scheduling algorithms and persistence are separate.
use astersql_pd_configutil::{
    ConfigMetaData, adjust_duration, adjust_float64, adjust_int64, adjust_string, adjust_uint64,
};
use astersql_pd_typeutil::{Duration, StringSlice};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// Pure values from Go ScheduleConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduleConfig {
    #[serde(rename = "max-snapshot-count")]
    #[serde(deserialize_with = "null_default")]
    pub max_snapshot_count: u64,
    #[serde(rename = "max-pending-peer-count")]
    #[serde(deserialize_with = "null_default")]
    pub max_pending_peer_count: u64,
    #[serde(rename = "max-merge-region-size")]
    #[serde(deserialize_with = "null_default")]
    pub max_merge_region_size: u64,
    #[serde(rename = "max-merge-region-keys")]
    #[serde(deserialize_with = "null_default")]
    pub max_merge_region_keys: u64,
    #[serde(rename = "split-merge-interval")]
    pub split_merge_interval: Duration,
    #[serde(rename = "switch-witness-interval")]
    pub switch_witness_interval: Duration,
    #[serde(rename = "enable-one-way-merge", with = "string_bool")]
    pub enable_one_way_merge: bool,
    #[serde(rename = "enable-cross-table-merge", with = "string_bool")]
    pub enable_cross_table_merge: bool,
    #[serde(rename = "patrol-region-interval")]
    pub patrol_region_interval: Duration,
    #[serde(rename = "max-store-down-time")]
    pub max_store_down_time: Duration,
    #[serde(rename = "max-store-preparing-time")]
    pub max_store_preparing_time: Duration,
    #[serde(rename = "leader-schedule-limit")]
    #[serde(deserialize_with = "null_default")]
    pub leader_schedule_limit: u64,
    #[serde(rename = "leader-schedule-policy")]
    #[serde(deserialize_with = "null_default")]
    pub leader_schedule_policy: String,
    #[serde(rename = "region-schedule-limit")]
    #[serde(deserialize_with = "null_default")]
    pub region_schedule_limit: u64,
    #[serde(rename = "witness-schedule-limit")]
    #[serde(deserialize_with = "null_default")]
    pub witness_schedule_limit: u64,
    #[serde(rename = "replica-schedule-limit")]
    #[serde(deserialize_with = "null_default")]
    pub replica_schedule_limit: u64,
    #[serde(rename = "merge-schedule-limit")]
    #[serde(deserialize_with = "null_default")]
    pub merge_schedule_limit: u64,
    #[serde(rename = "hot-region-schedule-limit")]
    #[serde(deserialize_with = "null_default")]
    pub hot_region_schedule_limit: u64,
    #[serde(rename = "affinity-schedule-limit")]
    #[serde(deserialize_with = "null_default")]
    pub affinity_schedule_limit: u64,
    #[serde(rename = "split-scatter-schedule-limit")]
    #[serde(deserialize_with = "null_default")]
    pub split_scatter_schedule_limit: u64,
    #[serde(rename = "hot-region-cache-hits-threshold")]
    #[serde(deserialize_with = "null_default")]
    pub hot_region_cache_hits_threshold: u64,
    #[serde(
        rename = "store-balance-rate",
        skip_serializing_if = "is_zero_float",
        serialize_with = "finite_float"
    )]
    #[serde(deserialize_with = "null_default")]
    pub store_balance_rate: f64,
    #[serde(rename = "default-store-limit")]
    #[serde(deserialize_with = "null_default")]
    pub default_store_limit: StoreLimitConfig,
    #[serde(rename = "store-limit")]
    pub store_limit: Option<BTreeMap<u64, StoreLimitConfig>>,
    #[serde(rename = "tolerant-size-ratio", serialize_with = "finite_float")]
    #[serde(deserialize_with = "null_default")]
    pub tolerant_size_ratio: f64,
    #[serde(rename = "low-space-ratio", serialize_with = "finite_float")]
    #[serde(deserialize_with = "null_default")]
    pub low_space_ratio: f64,
    #[serde(rename = "high-space-ratio", serialize_with = "finite_float")]
    #[serde(deserialize_with = "null_default")]
    pub high_space_ratio: f64,
    #[serde(rename = "region-score-formula-version")]
    #[serde(deserialize_with = "null_default")]
    pub region_score_formula_version: String,
    #[serde(rename = "scheduler-max-waiting-operator")]
    #[serde(deserialize_with = "null_default")]
    pub scheduler_max_waiting_operator: u64,
    #[serde(
        rename = "disable-raft-learner",
        with = "string_bool",
        skip_serializing_if = "is_false"
    )]
    pub disable_learner: bool,
    #[serde(
        rename = "disable-remove-down-replica",
        with = "string_bool",
        skip_serializing_if = "is_false"
    )]
    pub disable_remove_down_replica: bool,
    #[serde(
        rename = "disable-replace-offline-replica",
        with = "string_bool",
        skip_serializing_if = "is_false"
    )]
    pub disable_replace_offline_replica: bool,
    #[serde(
        rename = "disable-make-up-replica",
        with = "string_bool",
        skip_serializing_if = "is_false"
    )]
    pub disable_make_up_replica: bool,
    #[serde(
        rename = "disable-remove-extra-replica",
        with = "string_bool",
        skip_serializing_if = "is_false"
    )]
    pub disable_remove_extra_replica: bool,
    #[serde(
        rename = "disable-location-replacement",
        with = "string_bool",
        skip_serializing_if = "is_false"
    )]
    pub disable_location_replacement: bool,
    #[serde(rename = "enable-remove-down-replica", with = "string_bool")]
    pub enable_remove_down_replica: bool,
    #[serde(rename = "enable-replace-offline-replica", with = "string_bool")]
    pub enable_replace_offline_replica: bool,
    #[serde(rename = "enable-make-up-replica", with = "string_bool")]
    pub enable_make_up_replica: bool,
    #[serde(rename = "enable-remove-extra-replica", with = "string_bool")]
    pub enable_remove_extra_replica: bool,
    #[serde(rename = "enable-location-replacement", with = "string_bool")]
    pub enable_location_replacement: bool,
    #[serde(rename = "enable-debug-metrics", with = "string_bool")]
    pub enable_debug_metrics: bool,
    #[serde(rename = "enable-joint-consensus", with = "string_bool")]
    pub enable_joint_consensus: bool,
    #[serde(rename = "enable-tikv-split-region", with = "string_bool")]
    pub enable_ti_kv_split_region: bool,
    #[serde(rename = "enable-heartbeat-breakdown-metrics", with = "string_bool")]
    pub enable_heartbeat_breakdown_metrics: bool,
    #[serde(rename = "enable-heartbeat-concurrent-runner", with = "string_bool")]
    pub enable_heartbeat_concurrent_runner: bool,
    #[serde(rename = "schedulers-v2")]
    pub schedulers: Option<Vec<SchedulerConfig>>,
    #[serde(rename = "hot-regions-write-interval")]
    pub hot_regions_write_interval: Duration,
    #[serde(rename = "hot-regions-reserved-days")]
    #[serde(deserialize_with = "null_default")]
    pub hot_regions_reserved_days: u64,
    #[serde(
        rename = "max-movable-hot-peer-size",
        skip_serializing_if = "is_zero_int"
    )]
    #[serde(deserialize_with = "null_default")]
    pub max_movable_hot_peer_size: i64,
    #[serde(rename = "enable-diagnostic", with = "string_bool")]
    pub enable_diagnostic: bool,
    #[serde(rename = "enable-witness", with = "string_bool")]
    pub enable_witness: bool,
    #[serde(
        rename = "slow-store-evicting-affected-store-ratio-threshold",
        skip_serializing_if = "is_zero_float",
        serialize_with = "finite_float"
    )]
    #[serde(deserialize_with = "null_default")]
    pub slow_store_evicting_affected_store_ratio_threshold: f64,
    #[serde(
        rename = "store-limit-version",
        skip_serializing_if = "String::is_empty"
    )]
    #[serde(deserialize_with = "null_default")]
    pub store_limit_version: String,
    #[serde(
        rename = "halt-scheduling",
        with = "string_bool",
        skip_serializing_if = "is_false"
    )]
    pub halt_scheduling: bool,
    #[serde(rename = "patrol-region-worker-count")]
    #[serde(deserialize_with = "null_default")]
    pub patrol_region_worker_count: i64,
    #[serde(rename = "max-affinity-merge-region-size")]
    #[serde(deserialize_with = "null_default")]
    pub max_affinity_merge_region_size: u64,
}
/// Pure values from Go StoreLimitConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreLimitConfig {
    #[serde(rename = "add-peer", serialize_with = "finite_float")]
    #[serde(deserialize_with = "null_default")]
    pub add_peer: f64,
    #[serde(rename = "remove-peer", serialize_with = "finite_float")]
    #[serde(deserialize_with = "null_default")]
    pub remove_peer: f64,
    #[serde(rename = "transfer-leader-in", serialize_with = "finite_float")]
    #[serde(deserialize_with = "null_default")]
    pub transfer_leader_in: f64,
}
/// Pure values from Go SchedulerConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SchedulerConfig {
    #[serde(rename = "type")]
    #[serde(deserialize_with = "null_default")]
    pub r#type: String,
    #[serde(rename = "args")]
    pub args: Option<Vec<String>>,
    #[serde(rename = "disable")]
    #[serde(deserialize_with = "null_default")]
    pub disable: bool,
    #[serde(rename = "args-payload")]
    #[serde(deserialize_with = "null_default")]
    pub args_payload: String,
}
/// Pure values from Go ReplicationConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReplicationConfig {
    #[serde(rename = "max-replicas")]
    #[serde(deserialize_with = "null_default")]
    pub max_replicas: u64,
    #[serde(rename = "location-labels")]
    pub location_labels: StringSlice,
    #[serde(rename = "strictly-match-label", with = "string_bool")]
    pub strictly_match_label: bool,
    #[serde(rename = "enable-placement-rules", with = "string_bool")]
    pub enable_placement_rules: bool,
    #[serde(rename = "enable-placement-rules-cache", with = "string_bool")]
    pub enable_placement_rules_cache: bool,
    #[serde(rename = "isolation-level")]
    #[serde(deserialize_with = "null_default")]
    pub isolation_level: String,
}

fn is_false(v: &bool) -> bool {
    !v
}
fn is_zero_float(v: &f64) -> bool {
    *v == 0.0
}
fn is_zero_int(v: &i64) -> bool {
    *v == 0
}
fn finite_float<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if !v.is_finite() {
        return Err(serde::ser::Error::custom("unsupported non-finite float"));
    }
    if v.fract() == 0.0 && v.abs() < (1u64 << 53) as f64 {
        s.serialize_i64(*v as i64)
    } else {
        s.serialize_f64(*v)
    }
}
mod string_bool {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &bool, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(if *v { "true" } else { "false" })
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
        match Option::<String>::deserialize(d)?.as_deref() {
            Some("true") => Ok(true),
            Some("false") | None => Ok(false),
            _ => Err(serde::de::Error::custom("expected true or false")),
        }
    }
}

/// Defaults captured by the host; migrations update this snapshot for subsequent configs.
#[derive(Clone, Debug)]
pub struct ScheduleDefaults {
    pub store_limit: StoreLimitConfig,
    pub schedulers: Vec<SchedulerConfig>,
}
impl Default for ScheduleDefaults {
    fn default() -> Self {
        Self {
            store_limit: StoreLimitConfig {
                add_peer: 15.0,
                remove_peer: 15.0,
                transfer_leader_in: 100_000_000.0,
            },
            schedulers: [
                "balance-region",
                "balance-leader",
                "hot-region",
                "evict-slow-store",
                "evict-stopping-store",
            ]
            .into_iter()
            .map(|t| SchedulerConfig {
                r#type: t.into(),
                ..Default::default()
            })
            .collect(),
        }
    }
}
impl ScheduleConfig {
    /// Applies defaults and validates. The host owns process defaults; no global state is installed.
    pub fn adjust(
        &mut self,
        meta: &ConfigMetaData,
        reloading: bool,
        defaults: &mut ScheduleDefaults,
    ) -> Result<(), String> {
        if !meta.is_defined("max-snapshot-count") {
            adjust_uint64(&mut self.max_snapshot_count, 64);
        }
        if !meta.is_defined("max-pending-peer-count") {
            adjust_uint64(&mut self.max_pending_peer_count, 64);
        }
        if !meta.is_defined("max-merge-region-size") {
            adjust_uint64(&mut self.max_merge_region_size, 54);
        }
        if !meta.is_defined("max-affinity-merge-region-size") {
            adjust_uint64(&mut self.max_affinity_merge_region_size, 256);
        }
        adjust_duration(
            &mut self.split_merge_interval,
            Duration::from_nanos(3600000000000),
        );
        adjust_duration(
            &mut self.switch_witness_interval,
            Duration::from_nanos(3600000000000),
        );
        adjust_duration(
            &mut self.patrol_region_interval,
            Duration::from_nanos(10000000),
        );
        adjust_duration(
            &mut self.max_store_down_time,
            Duration::from_nanos(1800000000000),
        );
        adjust_duration(
            &mut self.hot_regions_write_interval,
            Duration::from_nanos(600000000000),
        );
        if !meta.is_defined("max-store-preparing-time") {
            adjust_duration(
                &mut self.max_store_preparing_time,
                Duration::from_nanos(172800000000000),
            );
        }
        if !meta.is_defined("leader-schedule-limit") {
            adjust_uint64(&mut self.leader_schedule_limit, 4);
        }
        if !meta.is_defined("region-schedule-limit") {
            adjust_uint64(&mut self.region_schedule_limit, 2048);
        }
        if !meta.is_defined("witness-schedule-limit") {
            adjust_uint64(&mut self.witness_schedule_limit, 4);
        }
        if !meta.is_defined("replica-schedule-limit") {
            adjust_uint64(&mut self.replica_schedule_limit, 64);
        }
        if !meta.is_defined("merge-schedule-limit") {
            adjust_uint64(&mut self.merge_schedule_limit, 8);
        }
        if !meta.is_defined("hot-region-schedule-limit") {
            adjust_uint64(&mut self.hot_region_schedule_limit, 4);
        }
        if !meta.is_defined("affinity-schedule-limit") {
            adjust_uint64(&mut self.affinity_schedule_limit, 0);
        }
        if !meta.is_defined("split-scatter-schedule-limit") {
            adjust_uint64(&mut self.split_scatter_schedule_limit, 4);
        }
        if !meta.is_defined("hot-region-cache-hits-threshold") {
            adjust_uint64(&mut self.hot_region_cache_hits_threshold, 3);
        }
        if !meta.is_defined("tolerant-size-ratio") {
            adjust_float64(&mut self.tolerant_size_ratio, 0.0);
        }
        if !meta.is_defined("scheduler-max-waiting-operator") {
            adjust_uint64(&mut self.scheduler_max_waiting_operator, 5);
        }
        if !meta.is_defined("leader-schedule-policy") {
            adjust_string(&mut self.leader_schedule_policy, "count");
        }
        if !meta.is_defined("store-limit-version") {
            adjust_string(&mut self.store_limit_version, "v1");
        }
        if !meta.is_defined("patrol-region-worker-count") {
            adjust_int64(&mut self.patrol_region_worker_count, 1);
        }
        if !meta.is_defined("enable-joint-consensus") {
            self.enable_joint_consensus = true;
        }
        if !meta.is_defined("enable-tikv-split-region") {
            self.enable_ti_kv_split_region = true;
        }
        if !meta.is_defined("enable-heartbeat-breakdown-metrics") {
            self.enable_heartbeat_breakdown_metrics = true;
        }
        if !meta.is_defined("enable-heartbeat-concurrent-runner") {
            self.enable_heartbeat_concurrent_runner = true;
        }
        if !meta.is_defined("enable-cross-table-merge") {
            self.enable_cross_table_merge = true;
        }
        adjust_float64(&mut self.low_space_ratio, 0.8);
        adjust_float64(&mut self.high_space_ratio, 0.7);
        if !meta.is_defined("enable-diagnostic") {
            self.enable_diagnostic = true;
        }
        if !meta.is_defined("enable-witness") {
            self.enable_witness = false;
        }
        if !meta.is_defined("region-score-formula-version") && !reloading {
            adjust_string(&mut self.region_score_formula_version, "v2");
        }
        if !meta.is_defined("halt-scheduling") {
            self.halt_scheduling = false;
        }
        if self.schedulers.as_ref().is_none_or(Vec::is_empty) {
            self.schedulers = Some(defaults.schedulers.clone());
        }
        let limits_meta = meta.child(["default-store-limit"]);
        if self.store_balance_rate != 0.0 {
            if !limits_meta.is_defined("add-peer") {
                self.default_store_limit.add_peer = self.store_balance_rate;
            }
            if !limits_meta.is_defined("remove-peer") {
                self.default_store_limit.remove_peer = self.store_balance_rate;
            }
            defaults.store_limit.add_peer = self.default_store_limit.add_peer;
            defaults.store_limit.remove_peer = self.default_store_limit.remove_peer;
            self.store_balance_rate = 0.0;
        }
        if !limits_meta.is_defined("add-peer") {
            adjust_float64(
                &mut self.default_store_limit.add_peer,
                defaults.store_limit.add_peer,
            );
        }
        if !limits_meta.is_defined("remove-peer") {
            adjust_float64(
                &mut self.default_store_limit.remove_peer,
                defaults.store_limit.remove_peer,
            );
        }
        if !limits_meta.is_defined("transfer-leader-in") {
            adjust_float64(
                &mut self.default_store_limit.transfer_leader_in,
                defaults.store_limit.transfer_leader_in,
            );
        }
        self.enable_remove_down_replica = deprecated_flag(
            meta,
            "remove-down-replica",
            self.disable_remove_down_replica,
            self.enable_remove_down_replica,
        )?;
        self.disable_remove_down_replica = false;
        self.enable_replace_offline_replica = deprecated_flag(
            meta,
            "replace-offline-replica",
            self.disable_replace_offline_replica,
            self.enable_replace_offline_replica,
        )?;
        self.disable_replace_offline_replica = false;
        self.enable_make_up_replica = deprecated_flag(
            meta,
            "make-up-replica",
            self.disable_make_up_replica,
            self.enable_make_up_replica,
        )?;
        self.disable_make_up_replica = false;
        self.enable_remove_extra_replica = deprecated_flag(
            meta,
            "remove-extra-replica",
            self.disable_remove_extra_replica,
            self.enable_remove_extra_replica,
        )?;
        self.disable_remove_extra_replica = false;
        self.enable_location_replacement = deprecated_flag(
            meta,
            "location-replacement",
            self.disable_location_replacement,
            self.enable_location_replacement,
        )?;
        self.disable_location_replacement = false;
        self.store_limit.get_or_insert_default();
        if !meta.is_defined("hot-regions-reserved-days") {
            adjust_uint64(&mut self.hot_regions_reserved_days, 7);
        }
        if !meta.is_defined("max-movable-hot-peer-size") {
            adjust_int64(&mut self.max_movable_hot_peer_size, 512);
        }
        if !meta.is_defined("slow-store-evicting-affected-store-ratio-threshold") {
            adjust_float64(
                &mut self.slow_store_evicting_affected_store_ratio_threshold,
                0.3,
            );
        }
        self.validate()
    }
    /// Validates the finite store rates, space ratios, policy and worker count.
    pub fn validate(&self) -> Result<(), String> {
        validate_limit(&self.default_store_limit, "default-store-limit")?;
        if let Some(limits) = &self.store_limit {
            for (id, limit) in limits {
                validate_limit(limit, &format!("store-limit[{id}]"))?;
            }
        }
        if self.tolerant_size_ratio < 0.0 {
            return Err("tolerant-size-ratio should be non-negative".into());
        }
        if self.low_space_ratio < 0.0 || self.low_space_ratio > 1.0 {
            return Err("low-space-ratio should between 0 and 1".into());
        }
        if self.high_space_ratio < 0.0 || self.high_space_ratio > 1.0 {
            return Err("high-space-ratio should between 0 and 1".into());
        }
        if self.low_space_ratio <= self.high_space_ratio {
            return Err("low-space-ratio should be larger than high-space-ratio".into());
        }
        if self.leader_schedule_policy != "count" && self.leader_schedule_policy != "size" {
            return Err(format!(
                "leader-schedule-policy {} is invalid",
                self.leader_schedule_policy
            ));
        }
        if self.slow_store_evicting_affected_store_ratio_threshold == 0.0 {
            return Err("slow-store-evicting-affected-store-ratio-threshold is not set".into());
        }
        if !(1..=8).contains(&self.patrol_region_worker_count) {
            return Err("patrol-region-worker-count should be between 1 and 8".into());
        }
        Ok(())
    }
}
fn validate_limit(limit: &StoreLimitConfig, prefix: &str) -> Result<(), String> {
    for (key, rate) in [
        ("add-peer", limit.add_peer),
        ("remove-peer", limit.remove_peer),
        ("transfer-leader-in", limit.transfer_leader_in),
    ] {
        if !rate.is_finite() || rate < 0.0 {
            return Err(format!("{prefix}.{key} should be finite and non-negative"));
        }
    }
    Ok(())
}
fn deprecated_flag(
    meta: &ConfigMetaData,
    name: &str,
    old: bool,
    new: bool,
) -> Result<bool, String> {
    let old_key = format!("disable-{name}");
    let new_key = format!("enable-{name}");
    match (meta.is_defined(&old_key), meta.is_defined(&new_key)) {
        (true, true) if old == new => Err(format!(
            "config item {new_key} and {old_key}(deprecated) are conflict"
        )),
        (_, true) => Ok(new),
        (true, false) => Ok(!old),
        _ => Ok(true),
    }
}
impl ReplicationConfig {
    /// Applies only Go ReplicationConfig.Adjust's value contract.
    pub fn adjust(&mut self, meta: &ConfigMetaData) -> Result<(), String> {
        adjust_uint64(&mut self.max_replicas, 3);
        if !meta.is_defined("enable-placement-rules") {
            self.enable_placement_rules = true;
        }
        if !meta.is_defined("strictly-match-label") {
            self.strictly_match_label = false;
        }
        if !meta.is_defined("location-labels") {
            self.location_labels = StringSlice::default();
        }
        self.validate()
    }
    /// Validates label syntax and isolation membership, without topology IO.
    pub fn validate(&self) -> Result<(), String> {
        for label in self.location_labels.as_slice() {
            validate_label_key(label)?;
        }
        if !self.isolation_level.is_empty()
            && !self
                .location_labels
                .as_slice()
                .contains(&self.isolation_level)
        {
            return Err("isolation-level must be one of location-labels or empty".into());
        }
        Ok(())
    }
}
/// Go's ASCII label-key grammar, including its optional leading dollar.
pub fn validate_label_key(key: &str) -> Result<(), String> {
    let plain = key.strip_prefix('$').unwrap_or(key);
    if !plain.is_empty()
        && plain.as_bytes()[0].is_ascii_alphanumeric()
        && plain.as_bytes()[plain.len() - 1].is_ascii_alphanumeric()
        && plain
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
    {
        return Ok(());
    }
    Err(format!(
        "{key} does not match format \"^[$]?[A-Za-z0-9]([-A-Za-z0-9_./]*[A-Za-z0-9])?$\""
    ))
}

/// Decodes a fresh scheduling JSON snapshot. New stores inherit omitted fields
/// from default-store-limit, including null fields, like Go UnmarshalJSON.
pub fn decode_json(input: &str) -> Result<ScheduleConfig, serde_json::Error> {
    let mut decoder = serde_json::Deserializer::from_str(input);
    let config = deserialize_json(&mut decoder)?;
    decoder.end()?;
    Ok(config)
}
/// Serde adapter for Config's scheduling field.
pub fn deserialize_json<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<ScheduleConfig, D::Error> {
    let mut value = serde_json::Value::deserialize(d)?;
    if value.is_null() {
        return Ok(ScheduleConfig::default());
    }
    let defaults: StoreLimitConfig = serde_json::from_value(
        value
            .get("default-store-limit")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    )
    .map_err(serde::de::Error::custom)?;
    let default_fields = serde_json::to_value(defaults).map_err(serde::de::Error::custom)?;
    if let Some(stores) = value
        .get_mut("store-limit")
        .and_then(serde_json::Value::as_object_mut)
    {
        for store in stores.values_mut() {
            if store.is_null() {
                *store = default_fields.clone();
                continue;
            }
            if let Some(fields) = store.as_object_mut() {
                for (key, v) in default_fields.as_object().unwrap() {
                    if fields.get(key).is_none_or(serde_json::Value::is_null) {
                        fields.insert(key.clone(), v.clone());
                    }
                }
            }
        }
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

fn null_default<'de, T: serde::Deserialize<'de> + Default, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod config_test;

/// Decodes a scheduling TOML table, preserving native flags and non-finite
/// float values for Validate. Integer-key store maps follow Go's TOML rejection.
pub fn decode_toml(input: &str) -> Result<(ScheduleConfig, ConfigMetaData), String> {
    let mut table: toml::Table = toml::from_str(input).map_err(|e| e.to_string())?;
    let meta = metadata(&table);
    if table.contains_key("store-limit") {
        return Err("toml: cannot decode to a map with non-string key type (uint64 in \"map[uint64]config.StoreLimitConfig\")".into());
    }
    let mut floats = Vec::new();
    for key in [
        "store-balance-rate",
        "tolerant-size-ratio",
        "low-space-ratio",
        "high-space-ratio",
        "slow-store-evicting-affected-store-ratio-threshold",
    ] {
        if let Some(v) = table.remove(key) {
            floats.push((key, number(&v)?));
        }
    }
    let mut limit_floats = Vec::new();
    if let Some(limit) = table
        .get_mut("default-store-limit")
        .and_then(toml::Value::as_table_mut)
    {
        for key in ["add-peer", "remove-peer", "transfer-leader-in"] {
            if let Some(v) = limit.remove(key) {
                limit_floats.push((key, number(&v)?));
            }
        }
    }
    let mut json = serde_json::to_value(table).map_err(|e| e.to_string())?;
    let map = json.as_object_mut().unwrap();
    if let Some(v) = map.remove("schedulers") {
        map.insert("schedulers-v2".into(), v);
    }
    for key in SCHEDULE_FLAGS {
        if let Some(v) = map.get_mut(*key) {
            let b = v.as_bool().ok_or("expected TOML boolean")?;
            *v = serde_json::Value::String(b.to_string());
        }
    }
    let mut config: ScheduleConfig = serde_json::from_value(json).map_err(|e| e.to_string())?;
    for (key, v) in floats {
        match key {
            "store-balance-rate" => config.store_balance_rate = v,
            "tolerant-size-ratio" => config.tolerant_size_ratio = v,
            "low-space-ratio" => config.low_space_ratio = v,
            "high-space-ratio" => config.high_space_ratio = v,
            _ => config.slow_store_evicting_affected_store_ratio_threshold = v,
        }
    }
    for (key, v) in limit_floats {
        match key {
            "add-peer" => config.default_store_limit.add_peer = v,
            "remove-peer" => config.default_store_limit.remove_peer = v,
            _ => config.default_store_limit.transfer_leader_in = v,
        }
    }
    Ok((config, meta))
}
/// Decodes the canonical replication table with its TOML array of labels.
pub fn decode_toml_replication(input: &str) -> Result<(ReplicationConfig, ConfigMetaData), String> {
    let mut table: toml::Table = toml::from_str(input).map_err(|e| e.to_string())?;
    let meta = metadata(&table);
    let labels: Option<Vec<String>> = table
        .remove("location-labels")
        .map(|v| v.try_into().map_err(|e: toml::de::Error| e.to_string()))
        .transpose()?;
    let mut json = serde_json::to_value(table).map_err(|e| e.to_string())?;
    for key in [
        "strictly-match-label",
        "enable-placement-rules",
        "enable-placement-rules-cache",
    ] {
        if let Some(v) = json.get_mut(key) {
            let b = v.as_bool().ok_or("expected TOML boolean")?;
            *v = serde_json::Value::String(b.to_string());
        }
    }
    let mut config: ReplicationConfig = serde_json::from_value(json).map_err(|e| e.to_string())?;
    if let Some(labels) = labels {
        config.location_labels = labels.into();
    }
    Ok((config, meta))
}
fn number(v: &toml::Value) -> Result<f64, String> {
    match v {
        toml::Value::Float(v) => Ok(*v),
        toml::Value::Integer(v) => Ok(*v as f64),
        _ => Err("expected TOML number".into()),
    }
}
fn metadata(table: &toml::Table) -> ConfigMetaData {
    let mut paths = Vec::new();
    for (key, value) in table {
        paths.push(vec![key.clone()]);
        if let Some(t) = value.as_table() {
            for child in t.keys() {
                paths.push(vec![key.clone(), child.clone()]);
            }
        }
    }
    ConfigMetaData::from_defined_paths(paths)
}
const SCHEDULE_FLAGS: &[&str] = &[
    "enable-one-way-merge",
    "enable-cross-table-merge",
    "disable-raft-learner",
    "disable-remove-down-replica",
    "disable-replace-offline-replica",
    "disable-make-up-replica",
    "disable-remove-extra-replica",
    "disable-location-replacement",
    "enable-remove-down-replica",
    "enable-replace-offline-replica",
    "enable-make-up-replica",
    "enable-remove-extra-replica",
    "enable-location-replacement",
    "enable-debug-metrics",
    "enable-joint-consensus",
    "enable-tikv-split-region",
    "enable-heartbeat-breakdown-metrics",
    "enable-heartbeat-concurrent-runner",
    "enable-diagnostic",
    "enable-witness",
    "halt-scheduling",
];

#[cfg(test)]
mod config_values_test;

mod value_contract;
pub use value_contract::validate_labels;

mod store_config;
pub use store_config::{
    Coprocessor, MergeAgainError, RAFTSTORE_V2, ReadPool, Storage, StoreConfig, UnifiedReadPool,
};
#[cfg(test)]
mod store_config_test;
