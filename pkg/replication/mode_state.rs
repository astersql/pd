// Copyright 2026 AsterSQL.
use crate::{ReplicationModeConfig, normalize_replication_mode};
use astersql_pd_id::{Allocator, AllocatorError};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{EndpointError, StorageEndpoint};
use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize, de};
use std::{fmt, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// The Go JSON model; unknown state names survive loading for forward compatibility.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DRAutoSyncStatus {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub state_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub async_start: Option<DateTime<FixedOffset>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recover_start: Option<DateTime<FixedOffset>>,
    #[serde(skip_serializing_if = "is_zero")]
    pub total_regions: i64,
    #[serde(skip_serializing_if = "is_zero")]
    pub synced_regions: i64,
    #[serde(skip_serializing_if = "is_zero", serialize_with = "finite_progress")]
    pub recover_progress: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub available_stores: Vec<u64>,
}

fn is_zero<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn finite_progress<S: serde::Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return Err(serde::ser::Error::custom(
            "unsupported non-finite recovery progress",
        ));
    }
    serializer.serialize_f32(*value)
}

// Go accepts null fields, case-folded JSON names and last duplicate wins, but
// rejects a positional array. Preserve those semantics at the storage boundary.
impl<'de> Deserialize<'de> for DRAutoSyncStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = DRAutoSyncStatus;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a replication status object")
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut value = DRAutoSyncStatus::default();
                while let Some(key) = map.next_key::<String>()? {
                    // These are the two non-ASCII SimpleFold classes that
                    // intersect ASCII tag names (Go encoding/json/fold.go).
                    let folded: String = key
                        .chars()
                        .map(|c| match c {
                            'ſ' => 's',
                            'K' => 'k',
                            c => c.to_ascii_lowercase(),
                        })
                        .collect();
                    match folded.as_str() {
                        "state" => {
                            value.state = map.next_value::<Option<String>>()?.unwrap_or_default()
                        }
                        "state_id" => {
                            value.state_id = map.next_value::<Option<u64>>()?.unwrap_or_default()
                        }
                        "async_start" => value.async_start = map.next_value()?,
                        "recover_start" => value.recover_start = map.next_value()?,
                        "total_regions" => {
                            value.total_regions =
                                map.next_value::<Option<i64>>()?.unwrap_or_default()
                        }
                        "synced_regions" => {
                            value.synced_regions =
                                map.next_value::<Option<i64>>()?.unwrap_or_default()
                        }
                        "recover_progress" => {
                            value.recover_progress =
                                map.next_value::<Option<f32>>()?.unwrap_or_default();
                            if !value.recover_progress.is_finite() {
                                return Err(de::Error::custom("recovery progress out of range"));
                            }
                        }
                        "available_stores" => {
                            value.available_stores =
                                map.next_value::<Option<Vec<u64>>>()?.unwrap_or_default()
                        }
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(value)
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

/// Configuration and DR state observed at the same publication boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct ModeSnapshot {
    pub config: ReplicationModeConfig,
    pub dr_auto_sync: DRAutoSyncStatus,
}

/// Typed domain errors; adapters own protocol status and error rendering.
#[derive(Debug, thiserror::Error)]
pub enum ModeError {
    #[error("invalid replication mode: {0}")]
    InvalidMode(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error(transparent)]
    Storage(#[from] EndpointError),
    #[error(transparent)]
    Allocation(#[from] AllocatorError),
}

/// Host-supplied wall time for persisted transition timestamps.
pub type ModeClock = Arc<dyn Fn() -> DateTime<FixedOffset> + Send + Sync>;

/// A durable policy manager sharing the host's metadata store and global allocator.
pub struct ModeManager {
    pub(crate) state: Mutex<ModeSnapshot>,
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    allocator: Arc<Allocator>,
    pub(crate) cancel: CancellationToken,
    clock: ModeClock,
}

impl ModeManager {
    /// Construct a manager without starting a worker or owning a runtime.
    pub async fn new(
        config: ReplicationModeConfig,
        storage: Arc<StorageEndpoint>,
        paths: ClusterPaths,
        allocator: Arc<Allocator>,
        cancel: CancellationToken,
    ) -> Result<Self, ModeError> {
        Self::new_with_clock(
            config,
            storage,
            paths,
            allocator,
            cancel,
            Arc::new(|| Utc::now().fixed_offset()),
        )
        .await
    }

    /// Restore DR state, initializing only when storage reports physical absence.
    pub async fn new_with_clock(
        config: ReplicationModeConfig,
        storage: Arc<StorageEndpoint>,
        paths: ClusterPaths,
        allocator: Arc<Allocator>,
        cancel: CancellationToken,
        clock: ModeClock,
    ) -> Result<Self, ModeError> {
        validate(&config)?;
        let mut manager = Self {
            state: Mutex::new(ModeSnapshot {
                config,
                dr_auto_sync: DRAutoSyncStatus::default(),
            }),
            storage,
            paths,
            allocator,
            cancel,
            clock,
        };
        if manager.cancel.is_cancelled() {
            return Err(ModeError::Cancelled);
        }
        if manager.state.get_mut().config.replication_mode == "dr-auto-sync" {
            let loaded = tokio::select! {
                biased;
                _ = manager.cancel.cancelled() => return Err(ModeError::Cancelled),
                result = manager.storage.load_replication_status::<DRAutoSyncStatus>(&manager.paths, "dr-auto-sync") => result?,
            };
            let status = match loaded {
                Some(status) => status,
                None => {
                    let status = manager.new_status("sync").await?;
                    manager.persist(&status).await?;
                    status
                }
            };
            manager.state.get_mut().dr_auto_sync = status;
        }
        Ok(manager)
    }

    /// Clone policy and state under one lock; no partially published transition is visible.
    pub async fn snapshot(&self) -> ModeSnapshot {
        self.state.lock().await.clone()
    }

    /// Enter recovery on the exact majority-to-DR transition, publishing after persistence.
    pub async fn update_config(&self, config: ReplicationModeConfig) -> Result<(), ModeError> {
        validate(&config)?;
        let mut state = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(ModeError::Cancelled),
            state = self.state.lock() => state,
        };
        if state.config.replication_mode == "majority" && config.replication_mode == "dr-auto-sync"
        {
            let status = self.new_status("sync_recover").await?;
            self.persist(&status).await?;
            state.dr_auto_sync = status;
        }
        state.config = config;
        Ok(())
    }

    pub(crate) async fn new_status(&self, state: &str) -> Result<DRAutoSyncStatus, ModeError> {
        let id = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(ModeError::Cancelled),
            result = self.allocator.alloc() => result?,
        };
        Ok(DRAutoSyncStatus {
            state: state.into(),
            state_id: id,
            recover_start: (state == "sync_recover").then(|| (self.clock)()),
            ..Default::default()
        })
    }

    pub(crate) async fn persist(&self, state: &DRAutoSyncStatus) -> Result<(), ModeError> {
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err(ModeError::Cancelled),
            result = self.storage.save_replication_status(&self.paths, "dr-auto-sync", state) => result.map_err(Into::into),
        }
    }
}

fn validate(config: &ReplicationModeConfig) -> Result<(), ModeError> {
    if normalize_replication_mode(&config.replication_mode).is_empty() {
        return Err(ModeError::InvalidMode(config.replication_mode.clone()));
    }
    Ok(())
}
