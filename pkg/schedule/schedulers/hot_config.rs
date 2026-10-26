// Copyright 2026 AsterSQL.
//! Hot scheduler configuration domain. HTTP and cluster-version detection stay in adapters.
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::{EndpointError, StorageEndpoint};
use astersql_pd_typeutil::Duration;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const HOT_REGION_CONFIG_NAME: &str = "hot-region-scheduler";
pub const TIFLASH_TOLERANCE_RATIO_CORRECTION: f64 = 0.1;

/// Which scheduling operation owns a priority list.
#[derive(Clone, Copy, Debug)]
pub enum HotPriorityScope {
    Read,
    WriteLeader,
    WritePeer,
}

/// Project stored priorities onto dimensions actually reported by the cluster.
/// This also repairs malformed old lists when displaying an effective snapshot;
/// it does not migrate or overwrite stored configuration.
pub fn adjust_hot_priorities(
    query_supported: bool,
    cpu_supported: bool,
    origins: &[String],
    scope: HotPriorityScope,
) -> Vec<String> {
    let compatible = match scope {
        HotPriorityScope::WriteLeader => ["key", "byte"],
        _ => ["byte", "key"],
    };
    let query = match scope {
        HotPriorityScope::Read | HotPriorityScope::WriteLeader => ["query", "byte"],
        HotPriorityScope::WritePeer => ["byte", "key"],
    };
    let defaults = match scope {
        HotPriorityScope::Read if cpu_supported => ["cpu", "byte"],
        _ => query,
    };
    let with_query = origins.iter().any(|s| s == "query");
    let with_cpu = origins.iter().any(|s| s == "cpu");
    let chosen = if !query_supported && (with_query || with_cpu) {
        compatible
    } else if !cpu_supported && with_cpu {
        query
    } else if origins.len() == 2
        && origins.iter().all(|s| is_dimension(s))
        && origins[0] != origins[1]
    {
        return origins.to_vec();
    } else if !query_supported {
        compatible
    } else {
        defaults
    };
    chosen.map(str::to_owned).to_vec()
}
fn is_dimension(s: &str) -> bool {
    matches!(s, "byte" | "key" | "query" | "cpu")
}

/// Current public hot-region parameters, using the Go JSON field names and durations.
/// No historical configuration migration is performed here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HotRegionParams {
    pub min_hot_byte_rate: f64,
    pub min_hot_key_rate: f64,
    pub min_hot_query_rate: f64,
    pub min_hot_cpu_rate: f64,
    pub max_zombie_rounds: i64,
    pub pending_weight: f64,
    pub max_peer_number: i64,
    pub byte_rate_rank_step_ratio: f64,
    pub key_rate_rank_step_ratio: f64,
    pub query_rate_rank_step_ratio: f64,
    pub cpu_rate_rank_step_ratio: f64,
    pub count_rank_step_ratio: f64,
    pub great_dec_ratio: f64,
    pub minor_dec_ratio: f64,
    pub src_tolerance_ratio: f64,
    pub dst_tolerance_ratio: f64,
    pub write_leader_priorities: Vec<String>,
    pub write_peer_priorities: Vec<String>,
    pub read_priorities: Vec<String>,
    #[serde(with = "string_bool")]
    pub strict_picking_store: bool,
    #[serde(with = "string_bool")]
    pub enable_for_tiflash: bool,
    pub rank_formula_version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub forbid_rw_type: String,
    pub split_thresholds: f64,
    pub history_sample_duration: Duration,
    pub history_sample_interval: Duration,
}
impl Default for HotRegionParams {
    fn default() -> Self {
        Self {
            min_hot_byte_rate: 100.,
            min_hot_key_rate: 10.,
            min_hot_query_rate: 10.,
            min_hot_cpu_rate: 10.,
            max_zombie_rounds: 3,
            pending_weight: 1.,
            max_peer_number: 1000,
            byte_rate_rank_step_ratio: 0.05,
            key_rate_rank_step_ratio: 0.05,
            query_rate_rank_step_ratio: 0.05,
            cpu_rate_rank_step_ratio: 0.05,
            count_rank_step_ratio: 0.01,
            great_dec_ratio: 0.95,
            minor_dec_ratio: 0.99,
            src_tolerance_ratio: 1.05,
            dst_tolerance_ratio: 1.05,
            write_leader_priorities: vec!["query".into(), "byte".into()],
            write_peer_priorities: vec!["byte".into(), "key".into()],
            read_priorities: vec!["cpu".into(), "byte".into()],
            strict_picking_store: true,
            enable_for_tiflash: true,
            rank_formula_version: "v2".into(),
            forbid_rw_type: "none".into(),
            split_thresholds: 0.2,
            history_sample_duration: Duration::from_nanos(
                astersql_pd_statistics::DEFAULT_HISTORY_SAMPLE_DURATION.as_nanos() as i64,
            ),
            history_sample_interval: Duration::from_nanos(
                astersql_pd_statistics::DEFAULT_HISTORY_SAMPLE_INTERVAL.as_nanos() as i64,
            ),
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum HotConfigError {
    #[error("{0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Storage(#[from] EndpointError),
}
impl HotRegionParams {
    pub fn validate(&self) -> Result<(), HotConfigError> {
        use HotConfigError::Invalid;
        for (scope, list) in [
            (HotPriorityScope::Read, &self.read_priorities),
            (HotPriorityScope::WriteLeader, &self.write_leader_priorities),
            (HotPriorityScope::WritePeer, &self.write_peer_priorities),
        ] {
            let mut seen = HashSet::new();
            for p in list {
                if !is_dimension(p) {
                    return Err(Invalid("invalid scheduling dimensions"));
                }
                seen.insert(p.as_str());
            }
            if seen.len() != list.len() {
                return Err(Invalid("priorities shouldn't be repeated"));
            }
            if seen.len() == 1 {
                return Err(Invalid("priorities should have at least 2 dimensions"));
            }
            match scope {
                HotPriorityScope::WriteLeader if seen.contains("cpu") => {
                    return Err(Invalid(
                        "cpu is not allowed to be set in priorities for write-leader-priorities",
                    ));
                }
                HotPriorityScope::WritePeer if seen.contains("cpu") => {
                    return Err(Invalid(
                        "cpu is not allowed to be set in priorities for write-peer-priorities",
                    ));
                }
                HotPriorityScope::WritePeer if seen.contains("query") => {
                    return Err(Invalid(
                        "query is not allowed to be set in priorities for write-peer-priorities",
                    ));
                }
                _ => {}
            }
        }
        if !matches!(self.rank_formula_version.as_str(), "" | "v1" | "v2") {
            return Err(Invalid("invalid rank-formula-version"));
        }
        if !matches!(self.forbid_rw_type.as_str(), "" | "none" | "read" | "write") {
            return Err(Invalid("invalid forbid-rw-type"));
        }
        if self.pending_weight < 1. {
            return Err(Invalid("invalid pending-weight, should be at least 1"));
        }
        if self.split_thresholds < 0.01 || self.split_thresholds > 1. {
            return Err(Invalid(
                "invalid split-thresholds, should be in range [0.01, 1.0]",
            ));
        }
        Ok(())
    }
    pub fn rank_version(&self) -> &str {
        if self.rank_formula_version == "v2" {
            "v2"
        } else {
            "v1"
        }
    }
    pub fn is_forbidden(&self, rw: &str) -> bool {
        matches!(rw, "read" | "write") && rw == self.forbid_rw_type
    }
    pub fn effective(&self, query: bool, cpu: bool) -> Self {
        let mut p = self.clone();
        p.read_priorities =
            adjust_hot_priorities(query, cpu, &self.read_priorities, HotPriorityScope::Read);
        p.write_leader_priorities = adjust_hot_priorities(
            query,
            cpu,
            &self.write_leader_priorities,
            HotPriorityScope::WriteLeader,
        );
        p.write_peer_priorities = adjust_hot_priorities(
            query,
            cpu,
            &self.write_peer_priorities,
            HotPriorityScope::WritePeer,
        );
        p.rank_formula_version = self.rank_version().into();
        if !matches!(p.forbid_rw_type.as_str(), "read" | "write") {
            p.forbid_rw_type.clear();
        }
        p
    }
    pub fn store_stat_zombie_duration(&self) -> Duration {
        Duration::from_nanos(self.max_zombie_rounds.wrapping_mul(10_000_000_000))
    }
    pub fn region_stat_zombie_duration(&self) -> Duration {
        Duration::from_nanos(self.max_zombie_rounds.wrapping_mul(60_000_000_000))
    }
}

/// Publication matches Go: a save failure is a warning and keeps the new live config.
#[derive(Debug)]
pub enum HotConfigUpdate {
    Unchanged,
    Updated {
        persistence_error: Option<EndpointError>,
    },
}
/// An owned config with exclusive update/reload access. Capability flags are transient.
#[derive(Default, Debug)]
pub struct HotRegionConfig {
    params: HotRegionParams,
    disabled: bool,
    query_supported: bool,
    cpu_supported: bool,
}
impl HotRegionConfig {
    pub fn new(params: HotRegionParams) -> Result<Self, HotConfigError> {
        params.validate()?;
        Ok(Self {
            params,
            ..Self::default()
        })
    }
    pub fn params(&self) -> &HotRegionParams {
        &self.params
    }
    pub fn disabled(&self) -> bool {
        self.disabled
    }
    /// The adapter obtains these flags from cluster version feature support.
    pub fn set_support(&mut self, query: bool, cpu: bool) {
        self.query_supported = query;
        self.cpu_supported = cpu;
    }
    pub fn valid_config(&self) -> HotRegionParams {
        self.params
            .effective(self.query_supported, self.cpu_supported)
    }
    pub fn encode(&self) -> Result<Vec<u8>, HotConfigError> {
        let mut value = serde_json::to_value(&self.params)?;
        value["disabled"] = Value::Bool(self.disabled);
        Ok(serde_json::to_vec(&value)?)
    }
    pub async fn update(
        &mut self,
        candidate: HotRegionParams,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<HotConfigUpdate, HotConfigError> {
        candidate.validate()?;
        // serde_json represents non-finite floats as null. Reject them before publication,
        // just as Go's JSON encoder does; no such values can enter through JSON patches.
        let candidate_value = serde_json::to_value(&candidate)?;
        if candidate_value
            .as_object()
            .unwrap()
            .values()
            .any(Value::is_null)
        {
            return Err(HotConfigError::Invalid("non-finite configuration number"));
        }
        if candidate == self.params {
            return Ok(HotConfigUpdate::Unchanged);
        }
        self.params = candidate;
        let bytes = self.encode()?;
        let persistence_error = storage
            .save_scheduler_config(paths, HOT_REGION_CONFIG_NAME, &bytes)
            .await
            .err();
        Ok(HotConfigUpdate::Updated { persistence_error })
    }
    /// Partial JSON update. Unknown-only/no-change input is an error; unknown fields
    /// alongside a real update are ignored, matching the Go handler's field lookup.
    pub async fn update_json(
        &mut self,
        bytes: &[u8],
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<HotConfigUpdate, HotConfigError> {
        let (candidate, known) = self.merge(bytes)?;
        candidate.validate()?;
        if candidate == self.params && !known {
            return Err(HotConfigError::Invalid("config item is not found"));
        }
        self.update(candidate, storage, paths).await
    }
    fn merge(&self, bytes: &[u8]) -> Result<(HotRegionParams, bool), HotConfigError> {
        let patch: Value = serde_json::from_slice(bytes)?;
        if patch.is_null() {
            return Ok((self.params.clone(), false));
        }
        let fields = patch
            .as_object()
            .ok_or(HotConfigError::Invalid("configuration must be an object"))?;
        let mut value = serde_json::to_value(&self.params)?;
        // omitempty still denotes a known field when currently empty.
        value["forbid-rw-type"] = Value::String(self.params.forbid_rw_type.clone());
        let mut known = false;
        for (key, incoming) in fields {
            // encoding/json matches exported tags case-insensitively.
            let matching = value
                .as_object()
                .unwrap()
                .keys()
                .find(|k| k.eq_ignore_ascii_case(key))
                .cloned();
            if let Some(key) = matching {
                // FindSameFieldByJSON uses exact JSON tags, unlike Unmarshal.
                known |= fields.contains_key(&key);
                if incoming.is_null() {
                    if key.ends_with("priorities") {
                        value[&key] = serde_json::json!([]);
                    }
                } else {
                    value[&key] = incoming.clone();
                }
            }
        }
        Ok((serde_json::from_value(value)?, known))
    }
    /// Reload overlays the active snapshot. Decode/validation/read failures leave it intact.
    /// Disabled is managed separately by the base scheduler, as in Go ReloadConfig.
    pub async fn reload(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), HotConfigError> {
        let bytes = storage
            .load_scheduler_config(paths, HOT_REGION_CONFIG_NAME)
            .await?;
        let (candidate, _) = self.merge(&bytes)?;
        candidate.validate()?;
        self.params = candidate;
        Ok(())
    }
}

mod string_bool {
    use serde::{Deserialize, Deserializer, Serializer, de};
    pub fn serialize<S: Serializer>(value: &bool, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(if *value { "true" } else { "false" })
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
        match String::deserialize(d)?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(de::Error::custom("invalid string boolean")),
        }
    }
}
