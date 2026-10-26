// Copyright 2026 AsterSQL.
//! Leader configuration lifecycle; HTTP decoding and status mapping belong to adapters.
use crate::{
    BalanceLeaderConfig, BalanceLeaderScheduler, SchedulerError, decode_config, encode_config,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::{EndpointError, StorageEndpoint};

/// Go publishes valid updates even when saving fails. The host must surface this warning;
/// a subsequent reload restores the last successfully persisted configuration.
#[derive(Debug)]
pub enum LeaderConfigUpdate {
    Unchanged,
    Updated {
        persistence_error: Option<EndpointError>,
    },
}
impl BalanceLeaderScheduler {
    pub fn encode_leader_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }

    /// Apply an owned, validated replacement. Exclusive access spans publication and I/O.
    /// An unchanged replacement does not write. Unlike SchedulerConfig::update, a save
    /// failure retains the published value, matching balanceLeaderSchedulerConfig.update.
    pub async fn update_leader_config(
        &mut self,
        candidate: BalanceLeaderConfig,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<LeaderConfigUpdate, SchedulerError> {
        let bytes = encode_config(&candidate)?;
        if bytes == self.encode_leader_config()? {
            return Ok(LeaderConfigUpdate::Unchanged);
        }
        self.config = candidate;
        let persistence_error = storage
            .save_scheduler_config(paths, &self.name, &bytes)
            .await
            .err();
        Ok(LeaderConfigUpdate::Updated { persistence_error })
    }

    /// Restore a validated snapshot from the canonical scheduler key. Load/decode errors
    /// leave the active snapshot unchanged; the scheduler name scopes storage lookup.
    pub async fn reload_leader_config(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), SchedulerError> {
        let bytes = storage.load_scheduler_config(paths, &self.name).await?;
        let candidate = decode_config(&bytes)?;
        self.config = candidate;
        Ok(())
    }
}
