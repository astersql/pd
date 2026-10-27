// Copyright 2026 AsterSQL.
//! Grant configuration recovery over canonical cluster pauses and MetaStore.
use crate::{
    GrantLeaderConfig, GrantLeaderScheduler, KeyRangeDecodeError, SchedulerError, decode_config,
    encode_config, get_key_ranges, pause_and_resume_leader_transfer,
};
use astersql_pd_core::{BasicCluster, KeyRange, StoreControlError};
use astersql_pd_core_constant::Direction;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::StorageEndpoint;
use std::collections::BTreeMap;
const NAME: &str = "grant-leader-scheduler";

#[derive(Debug, thiserror::Error)]
pub enum GrantRemoveError {
    #[error("scheduler not found")]
    NotFound,
    #[error("{0}")]
    Failed(String),
}
#[derive(Debug, thiserror::Error)]
pub enum GrantConfigError {
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
    Remove(#[from] GrantRemoveError),
}
/// Go persists range bytes as hexadecimal strings through canonical keyutil.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct GrantConfigJson {
    #[serde(rename = "store-id-ranges", default)]
    stores: Option<BTreeMap<u64, Option<Vec<astersql_pd_keyutil::KeyRange>>>>,
}
impl From<GrantConfigJson> for GrantLeaderConfig {
    fn from(value: GrantConfigJson) -> Self {
        Self::new(
            value
                .stores
                .unwrap_or_default()
                .into_iter()
                .map(|(id, ranges)| {
                    (
                        id,
                        ranges
                            .unwrap_or_default()
                            .into_iter()
                            .map(|r| KeyRange {
                                start_key: r.start_key,
                                end_key: r.end_key,
                            })
                            .collect(),
                    )
                })
                .collect(),
        )
    }
}
impl From<GrantLeaderConfig> for GrantConfigJson {
    fn from(value: GrantLeaderConfig) -> Self {
        Self {
            stores: Some(
                value
                    .stores
                    .into_iter()
                    .map(|(id, ranges)| {
                        (
                            id,
                            Some(
                                ranges
                                    .into_iter()
                                    .map(|r| {
                                        astersql_pd_keyutil::KeyRange::new(r.start_key, r.end_key)
                                    })
                                    .collect(),
                            ),
                        )
                    })
                    .collect(),
            ),
        }
    }
}
impl GrantLeaderConfig {
    /// Exact ParseUint base-10 args: one store followed by URL-decoded range pairs.
    /// An unmatched trailing key is ignored; no pairs means the full keyspace.
    pub fn from_args(args: &[&str]) -> Result<Self, GrantConfigError> {
        let first = args
            .first()
            .ok_or(GrantConfigError::Invalid("missing store id"))?;
        if first.is_empty() || !first.bytes().all(|b| b.is_ascii_digit()) {
            return Err(GrantConfigError::Invalid("invalid store id"));
        }
        let id = first
            .parse()
            .map_err(|_| GrantConfigError::Invalid("invalid store id"))?;
        Ok(Self::new(BTreeMap::from([(
            id,
            get_key_ranges(&args[1..])?,
        )])))
    }
    /// Remove only a configured store, releasing one OUT pause reference.
    pub fn remove_store(&mut self, id: u64, cluster: &BasicCluster) -> (bool, bool) {
        if self.stores.remove(&id).is_some() {
            cluster.stores.resume_leader_transfer(id, Direction::OUT);
            (true, self.stores.is_empty())
        } else {
            (false, false)
        }
    }
    /// Best-effort pause and unconditional restore match Go resetStore. Return
    /// the pause failure for host diagnostics without discarding the restored entry.
    pub fn reset_store(
        &mut self,
        id: u64,
        ranges: Vec<KeyRange>,
        cluster: &BasicCluster,
    ) -> Result<(), StoreControlError> {
        let result = cluster.stores.pause_leader_transfer(id, Direction::OUT);
        self.stores.insert(id, ranges);
        result
    }
}
/// Until publication a new pause is owned by this future, including cancellation.
struct NewPause<'a> {
    cluster: &'a BasicCluster,
    id: Option<u64>,
}
impl Drop for NewPause<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            self.cluster
                .stores
                .resume_leader_transfer(id, Direction::OUT);
        }
    }
}
impl GrantLeaderScheduler {
    pub fn encode_grant_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }
    /// Typed domain input after HTTP conversion. None reparses existing ranges;
    /// Some(empty) resets to full range. No extra pause for retained stores.
    /// Go's handler removes even an existing entry on a failed save, while leaving
    /// the previously persisted snapshot intact. Preserve that recovery behavior.
    pub async fn update_grant_config(
        &mut self,
        id: u64,
        ranges: Option<&[&str]>,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), GrantConfigError> {
        let exists = self.config.stores.contains_key(&id);
        let mut pause = NewPause { cluster, id: None };
        if !exists {
            cluster.stores.pause_leader_transfer(id, Direction::OUT)?;
            pause.id = Some(id);
        }
        // Go getRanges converts stored bytes to strings and buildWithArgs then
        // QueryUnescapes them again. Preserve '+'/'%' behavior and empty-list
        // defaulting while losslessly carrying arbitrary byte keys via &str.
        let retained: Vec<String> = self
            .config
            .stores
            .get(&id)
            .into_iter()
            .flatten()
            .flat_map(|r| [&r.start_key, &r.end_key])
            .map(|key| {
                let mut text = String::new();
                const HEX: &[u8; 16] = b"0123456789abcdef";
                for &byte in key {
                    if byte.is_ascii() {
                        text.push(char::from(byte));
                    } else {
                        text.push('%');
                        text.push(char::from(HEX[(byte >> 4) as usize]));
                        text.push(char::from(HEX[(byte & 15) as usize]));
                    }
                }
                text
            })
            .collect();
        let retained_refs: Vec<_> = retained.iter().map(String::as_str).collect();
        let ranges = match get_key_ranges(ranges.unwrap_or(&retained_refs)) {
            Ok(r) => r,
            Err(e) => {
                if exists {
                    cluster.stores.resume_leader_transfer(id, Direction::OUT);
                }
                return Err(e.into());
            }
        };
        let mut candidate = self.config.clone();
        candidate.stores.insert(id, ranges);
        if let Err(error) = storage
            .save_scheduler_config(paths, NAME, &encode_config(&candidate)?)
            .await
        {
            self.config.stores.remove(&id);
            if exists {
                cluster.stores.resume_leader_transfer(id, Direction::OUT);
            }
            return Err(SchedulerError::from(error).into());
        }
        self.config = candidate;
        pause.id = None;
        Ok(())
    }
    /// Failed persistence leaves the original map and pause. A failed final-store
    /// callback restores only memory/OUT pause, except NotFound leaves it removed.
    pub async fn delete_grant_store(
        &mut self,
        id: u64,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        remove: impl FnOnce(&str) -> Result<(), GrantRemoveError>,
    ) -> Result<Option<&'static str>, GrantConfigError> {
        let mut candidate = self.config.clone();
        candidate
            .stores
            .remove(&id)
            .ok_or(GrantConfigError::MissingStore(id))?;
        storage
            .save_scheduler_config(paths, NAME, &encode_config(&candidate)?)
            .await
            .map_err(SchedulerError::from)?;
        let previous = std::mem::replace(&mut self.config, candidate);
        cluster.stores.resume_leader_transfer(id, Direction::OUT);
        if !self.config.stores.is_empty() {
            return Ok(None);
        }
        if let Err(error) = remove(NAME) {
            if !matches!(error, GrantRemoveError::NotFound) {
                // Go logs pause errors; callback error remains the primary error.
                let _pause_error = cluster
                    .stores
                    .pause_leader_transfer(id, Direction::OUT)
                    .err();
                self.config = previous;
            }
            return Err(error.into());
        }
        Ok(Some("The last store has been deleted"))
    }
    /// Decode before changing pauses. Retained stores do not gain a reference;
    /// missing new stores are diagnostic failures while the loaded map publishes.
    pub async fn reload_grant_config(
        &mut self,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<Vec<StoreControlError>, SchedulerError> {
        let candidate: GrantLeaderConfig =
            decode_config(&storage.load_scheduler_config(paths, NAME).await?)?;
        let errors = pause_and_resume_leader_transfer(
            cluster,
            Direction::OUT,
            &self.config.stores.keys().copied().collect(),
            &candidate.stores.keys().copied().collect(),
        );
        self.config = candidate;
        Ok(errors)
    }
}
