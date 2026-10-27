// Copyright 2026 AsterSQL.
//! Eviction configuration transactions over canonical cluster controls and MetaStore.
use crate::{
    EvictLeaderConfig, EvictLeaderScheduler, KeyRangeDecodeError, SchedulerError, decode_config,
    encode_config, get_key_ranges, pause_and_resume_leader_transfer,
};
use astersql_pd_core::{BasicCluster, KeyRange, StoreControlError};
use astersql_pd_core_constant::Direction;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::StorageEndpoint;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
const NAME: &str = "evict-leader-scheduler";

#[derive(Debug, thiserror::Error)]
pub enum EvictRemoveError {
    #[error("scheduler not found")]
    NotFound,
    #[error("{0}")]
    Failed(String),
}
#[derive(Debug, thiserror::Error)]
pub enum EvictConfigError {
    #[error("{0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Range(#[from] KeyRangeDecodeError),
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error(transparent)]
    Pause(#[from] StoreControlError),
    #[error("schedule config does not exist for store {0}")]
    MissingStore(u64),
    #[error(transparent)]
    Remove(#[from] EvictRemoveError),
}
/// JSON numbers follow Go's float64 decoding. Reject non-integral/non-finite and
/// non-representable IDs before conversion, including the rounded 2^64 boundary.
pub fn evict_store_id_from_float(f: f64) -> Result<u64, EvictConfigError> {
    if !f.is_finite() || f < 0. || f.fract() != 0. || f >= 18446744073709551616. {
        return Err(EvictConfigError::Invalid("please input a right store id"));
    }
    Ok(f as u64)
}
fn parse_id(s: &str) -> Result<u64, EvictConfigError> {
    // strconv.ParseUint(base 10) permits digits only, including leading zeros.
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(EvictConfigError::Invalid("invalid store id"));
    }
    s.parse()
        .map_err(|_| EvictConfigError::Invalid("invalid store id"))
}
fn ranges(args: &[&str]) -> Result<Vec<KeyRange>, EvictConfigError> {
    get_key_ranges(args).map_err(EvictConfigError::Range)
}
impl EvictLeaderConfig {
    /// Creation args use exact uint64 parsing. Multi-store creation has one
    /// comma-joined argument and forbids accompanying ranges. Duplicate IDs merge.
    pub fn from_args(args: &[&str]) -> Result<Self, EvictConfigError> {
        let Some(first) = args.first() else {
            return Err(EvictConfigError::Invalid("missing store id"));
        };
        let stores = if let Ok(id) = parse_id(first) {
            BTreeMap::from([(id, ranges(&args[1..])?)])
        } else {
            if args.len() != 1 {
                return Err(EvictConfigError::Invalid("ranges with multiple store ids"));
            }
            first
                .split(',')
                .map(|s| Ok((parse_id(s)?, vec![KeyRange::default()])))
                .collect::<Result<_, EvictConfigError>>()?
        };
        Ok(Self::new(stores, 3).expect("default batch is valid"))
    }
}
struct Update {
    ids: Vec<u64>,
    ranges: Option<Vec<KeyRange>>,
    batch: usize,
}
impl Update {
    fn parse(input: &Value, batch: usize) -> Result<Self, EvictConfigError> {
        let object = input
            .as_object()
            .ok_or(EvictConfigError::Invalid("expected configuration object"))?;
        let one = object.get("store_id");
        let many = object.get("store_ids");
        if one.is_some() && many.is_some() {
            return Err(EvictConfigError::Invalid(
                "only one of store_id and store_ids can be set",
            ));
        }
        let id = |v: &Value| {
            evict_store_id_from_float(
                v.as_f64()
                    .ok_or(EvictConfigError::Invalid("please input a right store id"))?,
            )
        };
        let mut ids = if let Some(v) = one {
            vec![id(v)?]
        } else if let Some(v) = many {
            let a = v
                .as_array()
                .filter(|a| !a.is_empty())
                .ok_or(EvictConfigError::Invalid("please input a right store id"))?;
            a.iter().map(id).collect::<Result<Vec<_>, _>>()?
        } else {
            vec![]
        };
        let mut seen = HashSet::new();
        ids.retain(|id| seen.insert(*id));
        let batch = match object.get("batch").filter(|v| !v.is_null()) {
            None => batch,
            Some(v) => {
                let f = v
                    .as_f64()
                    .ok_or(EvictConfigError::Invalid("batch must be a number"))?;
                if !(1. ..=100.).contains(&f) || f.fract() != 0. {
                    return Err(EvictConfigError::Invalid(
                        "batch must be an integer in [1, 100]",
                    ));
                }
                f as usize
            }
        };
        let explicit = if let Some(v) = object.get("ranges") {
            if ids.is_empty() {
                return Err(EvictConfigError::Invalid("ranges require store ids"));
            }
            let a = v.as_array().ok_or(EvictConfigError::Invalid(
                "ranges must be an array of strings",
            ))?;
            let strs = a
                .iter()
                .map(|v| {
                    v.as_str().ok_or(EvictConfigError::Invalid(
                        "ranges must be an array of strings",
                    ))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if strs.is_empty() {
                None
            } else {
                Some(ranges(&strs)?)
            }
        } else {
            None
        };
        Ok(Self {
            ids,
            ranges: explicit,
            batch,
        })
    }
}
/// Roll back only pauses acquired by this call, including cancellation while
/// awaiting storage. Independent transfer-in/out pause reference counts survive.
struct Pauses<'a> {
    cluster: &'a BasicCluster,
    ids: Vec<u64>,
}
impl Drop for Pauses<'_> {
    fn drop(&mut self) {
        for id in &self.ids {
            self.cluster
                .stores
                .resume_leader_transfer(*id, Direction::IN);
        }
    }
}
impl EvictLeaderScheduler {
    pub fn encode_evict_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }

    /// Validate the entire request before side effects. Exclusive access spans
    /// pause acquisition, one persistent save and publication; failures retain the
    /// original batch/ranges and undo newly acquired pauses. Existing ranges are
    /// preserved when ranges are absent or empty, and copied per store otherwise.
    pub async fn update_evict_config(
        &mut self,
        input: &Value,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictConfigError> {
        let update = Update::parse(input, self.config.batch())?;
        let mut stores = self.config.stores().clone();
        let mut pauses = Pauses {
            cluster,
            ids: vec![],
        };
        for id in update.ids {
            if !stores.contains_key(&id) {
                cluster.stores.pause_leader_transfer(id, Direction::IN)?;
                pauses.ids.push(id);
                stores.insert(
                    id,
                    update
                        .ranges
                        .clone()
                        .unwrap_or_else(|| vec![KeyRange::default()]),
                );
            } else if let Some(ranges) = &update.ranges {
                stores.insert(id, ranges.clone());
            }
        }
        let candidate = EvictLeaderConfig::new(stores, update.batch).expect("parsed valid batch");
        storage
            .save_scheduler_config(paths, NAME, &encode_config(&candidate)?)
            .await
            .map_err(SchedulerError::from)?;
        self.config = candidate;
        pauses.ids.clear();
        Ok(())
    }
    /// Save a removal before publishing/resuming. The final-store callback runs
    /// after the empty configuration is persisted. Go restores only memory and
    /// its pause on callback errors other than NotFound; storage stays empty.
    pub async fn delete_evict_store(
        &mut self,
        id: u64,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        remove: impl FnOnce(&str) -> Result<(), EvictRemoveError>,
    ) -> Result<Option<&'static str>, EvictConfigError> {
        let mut stores = self.config.stores().clone();
        stores
            .remove(&id)
            .ok_or(EvictConfigError::MissingStore(id))?;
        let candidate =
            EvictLeaderConfig::new(stores, self.config.batch()).expect("existing valid batch");
        storage
            .save_scheduler_config(paths, NAME, &encode_config(&candidate)?)
            .await
            .map_err(SchedulerError::from)?;
        let previous = std::mem::replace(&mut self.config, candidate);
        cluster.stores.resume_leader_transfer(id, Direction::IN);
        if !self.config.stores().is_empty() {
            return Ok(None);
        }
        if let Err(error) = remove(NAME) {
            if !matches!(error, EvictRemoveError::NotFound) {
                // Go logs this pause error and restores the store regardless.
                let pause_error = cluster
                    .stores
                    .pause_leader_transfer(id, Direction::IN)
                    .err();
                self.config = previous;
                if let Some(error) = pause_error {
                    return Err(EvictConfigError::Pause(error));
                }
            }
            return Err(error.into());
        }
        Ok(Some("The last store has been deleted"))
    }
    /// Load and decode before side effects. Batch zero/missing defaults to 3.
    /// Added-store pause errors are returned to the host for logging while the
    /// loaded snapshot still publishes, matching Go's best-effort reconciliation.
    pub async fn reload_evict_config(
        &mut self,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<Vec<StoreControlError>, SchedulerError> {
        let bytes = storage.load_scheduler_config(paths, NAME).await?;
        let candidate: EvictLeaderConfig = decode_config(&bytes)?;
        let errors = pause_and_resume_leader_transfer(
            cluster,
            Direction::IN,
            &self.config.stores().keys().copied().collect(),
            &candidate.stores().keys().copied().collect(),
        );
        self.config = candidate;
        Ok(errors)
    }
}
