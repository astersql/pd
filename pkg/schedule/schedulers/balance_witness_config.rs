// Copyright 2026 AsterSQL.
//! Validated witness configuration persistence; HTTP decoding belongs to adapters.
use crate::{
    BalanceWitnessConfig, BalanceWitnessScheduler, SchedulerError, decode_config, encode_config,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::StorageEndpoint;

const CONFIG_NAME: &str = "balance-witness-scheduler";

impl BalanceWitnessScheduler {
    pub fn encode_witness_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }

    /// Persist an owned, validated replacement before publishing batch and ranges.
    /// Returns false for identical values without accessing storage. Save failures
    /// retain the active snapshot, as required by task 74 (unlike Go's update).
    pub async fn update_witness_config(
        &mut self,
        candidate: BalanceWitnessConfig,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<bool, SchedulerError> {
        if !(1..=10).contains(&candidate.transport_batch()) {
            return Err(SchedulerError::InvalidConfig(
                crate::WitnessConfigError.to_string(),
            ));
        }
        let bytes = encode_config(&candidate)?;
        if bytes == self.encode_witness_config()? {
            return Ok(false);
        }
        storage
            .save_scheduler_config(paths, CONFIG_NAME, &bytes)
            .await?;
        self.config = candidate;
        Ok(true)
    }

    /// Restore a fully validated snapshot. Load and decode failures leave the
    /// active batch and ranges unchanged. Exclusive access spans publication and I/O.
    pub async fn reload_witness_config(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), SchedulerError> {
        let bytes = storage.load_scheduler_config(paths, CONFIG_NAME).await?;
        let candidate = decode_config(&bytes)?;
        self.config = candidate;
        Ok(())
    }
}
