// Copyright 2026 AsterSQL.
use crate::{BoundType, build_label_rule, make_region_bound};
use astersql_pd_core::RegionsInfo;
use astersql_pd_errs::TypedError;
use astersql_pd_schedule_labeler::{LabelRule, LabelerError, RegionLabeler, Timestamp};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
/// Host-owned live wait configuration; the timeout is captured once per wait.
#[derive(Clone, Copy, Debug)]
pub struct KeyspaceSplitConfig {
    pub interval: Duration,
    pub timeout: Duration,
}
impl Default for KeyspaceSplitConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(50),
            timeout: Duration::from_secs(30),
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum KeyspaceSplitError {
    #[error(transparent)]
    Labeler(#[from] LabelerError),
    #[error(transparent)]
    Timeout(TypedError),
    #[error("[keyspace] wait region split canceled")]
    Cancelled,
    #[error("cluster does not support region label")]
    Unsupported,
}
/// Cleanup failure never replaces the original wait error.
#[derive(Debug)]
pub struct KeyspaceSplitFailure {
    pub error: KeyspaceSplitError,
    pub cleanup_error: Option<LabelerError>,
}
impl std::fmt::Display for KeyspaceSplitFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for KeyspaceSplitFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
/// Host adapter for the canonical serialized labeler; split/wait policy remains here.
#[async_trait::async_trait]
pub trait KeyspaceLabeler: Send + Sync {
    async fn set(&self, rule: LabelRule, now: Timestamp) -> Result<(), LabelerError>;
    async fn delete(&self, id: &str) -> Result<(), LabelerError>;
}
#[async_trait::async_trait]
impl KeyspaceLabeler for Mutex<RegionLabeler> {
    async fn set(&self, rule: LabelRule, now: Timestamp) -> Result<(), LabelerError> {
        self.lock().await.set_label_rule(rule, now).await
    }
    async fn delete(&self, id: &str) -> Result<(), LabelerError> {
        self.lock().await.delete_label_rule(id).await
    }
}
/// Registers boundaries on the canonical labeler and observes the shared region index.
#[derive(Clone)]
pub struct KeyspaceRegionSplit {
    labeler: Option<Arc<dyn KeyspaceLabeler>>,
    regions: Option<RegionsInfo>,
    config: Arc<RwLock<KeyspaceSplitConfig>>,
}
impl KeyspaceRegionSplit {
    pub fn new(
        labeler: Option<Arc<Mutex<RegionLabeler>>>,
        regions: Option<RegionsInfo>,
        config: Arc<RwLock<KeyspaceSplitConfig>>,
    ) -> Self {
        Self::with_labeler(
            labeler.map(|l| l as Arc<dyn KeyspaceLabeler>),
            regions,
            config,
        )
    }
    pub fn with_labeler(
        labeler: Option<Arc<dyn KeyspaceLabeler>>,
        regions: Option<RegionsInfo>,
        config: Arc<RwLock<KeyspaceSplitConfig>>,
    ) -> Self {
        Self {
            labeler,
            regions,
            config,
        }
    }
    /// Persist a rule first; waiting never holds the labeler's mutation lock.
    /// Cancellation belongs to the waiting owner; labeler persistence retains its host lifetime.
    pub async fn split(
        &self,
        cancel: &CancellationToken,
        id: u32,
        wait: bool,
        bound: BoundType,
        now: Timestamp,
    ) -> Result<(), KeyspaceSplitFailure> {
        let labeler = self.labeler.as_ref().ok_or_else(|| KeyspaceSplitFailure {
            error: KeyspaceSplitError::Unsupported,
            cleanup_error: None,
        })?;
        let rule = build_label_rule(id, bound);
        let rule_id = rule.id.clone();
        labeler
            .set(rule, now)
            .await
            .map_err(|error| KeyspaceSplitFailure {
                error: KeyspaceSplitError::Labeler(error),
                cleanup_error: None,
            })?;
        if wait && let Err(error) = self.wait_for_boundaries(cancel, id, bound).await {
            let cleanup_error = labeler.delete(&rule_id).await.err();
            return Err(KeyspaceSplitFailure {
                error,
                cleanup_error,
            });
        }
        Ok(())
    }
    /// Both lookups must resolve to regions whose actual start keys equal the boundaries.
    pub fn has_boundaries(&self, id: u32, bound: BoundType) -> bool {
        let boundaries = make_region_bound(id);
        let (left, right) = bound.bounds(&boundaries);
        self.regions.as_ref().is_some_and(|regions| {
            [left, right].into_iter().all(|key| {
                regions
                    .region_by_key(key, false)
                    .is_some_and(|region| region.meta().key_range.start_key == key)
            })
        })
    }
    /// Like Go, already present boundaries are checked on the first polling tick.
    /// Interval changes apply after an unsuccessful tick; the timeout is fixed at entry.
    pub async fn wait_for_boundaries(
        &self,
        cancel: &CancellationToken,
        id: u32,
        bound: BoundType,
    ) -> Result<(), KeyspaceSplitError> {
        let config = *self.config.read().unwrap();
        assert!(!config.interval.is_zero(), "poll interval must be positive");
        let tick = tokio::time::sleep(config.interval);
        let timeout = tokio::time::sleep(config.timeout);
        tokio::pin!(tick, timeout);
        loop {
            tokio::select! {
                _=cancel.cancelled()=>return Err(KeyspaceSplitError::Cancelled),
                _=&mut tick=>{
                    if self.has_boundaries(id,bound) {return Ok(());}
                    let interval=self.config.read().unwrap().interval;assert!(!interval.is_zero(),"poll interval must be positive");tick.as_mut().reset(tokio::time::Instant::now()+interval);
                },
                _=&mut timeout=>return Err(KeyspaceSplitError::Timeout(astersql_pd_errs::catalog::ERR_REGION_SPLIT_TIMEOUT.instantiate(Vec::<String>::new()))),
            }
        }
    }
}
