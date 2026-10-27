// Copyright 2026 AsterSQL.
//! Management split requests driven by the host's canonical snapshots and clock.
use crate::{Handler, HandlerError, admin_parse::decode_hex};
use astersql_pd_core::RegionsInfo;
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_splitter::{SplitRetry, SplitRetryPhase, SplitSubmissions};
use serde::{Deserialize, Serialize};

/// Domain response with the source field names and nil/empty slice distinction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitRegionsResponse {
    #[serde(rename = "processed-percentage")]
    pub processed_percentage: usize,
    #[serde(rename = "regions-id")]
    pub new_regions_id: Option<Vec<u64>>,
}
/// Bootstrap and decoding fail before any split operator is submitted.
#[derive(Debug, thiserror::Error)]
pub enum SplitAdminError {
    #[error(transparent)]
    Handler(#[from] HandlerError),
    #[error("{0}")]
    Hex(String),
}
/// Owned request state; dropping it owns no task, runtime, or timer to clean up.
/// The host dispatches submissions, supplies current canonical region snapshots,
/// and drives watch ticks and backoff, including the final sleep after cancellation.
pub struct SplitRegions {
    retry: SplitRetry,
    empty_input: bool,
}
impl SplitRegions {
    pub fn phase(&self) -> SplitRetryPhase {
        self.retry.phase()
    }
    pub fn advance(&mut self, now_ms: u64, cancelled: bool, regions: &RegionsInfo) {
        self.retry.advance(now_ms, cancelled, regions);
    }
    /// Only settled observations produce a management response. Admission does not.
    pub fn response(&self) -> Option<SplitRegionsResponse> {
        if self.phase() != SplitRetryPhase::Done {
            return None;
        }
        let result = self.retry.result();
        Some(SplitRegionsResponse {
            processed_percentage: result.percentage,
            new_regions_id: (!self.empty_input).then_some(result.region_ids),
        })
    }
}
impl<C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static> Handler<'_, C> {
    /// Bootstrap first, then decode all keys atomically, preserving order and duplicates.
    /// Retry limits retain the canonical splitter's inclusive signed semantics.
    pub fn split_regions(
        &self,
        split_key_hexes: &[impl AsRef<str>],
        retry_limit: i32,
    ) -> Result<SplitRegions, SplitAdminError> {
        self.coordinator()?;
        let keys = split_key_hexes
            .iter()
            .map(|key| decode_hex(key.as_ref()).map_err(|e| SplitAdminError::Hex(e.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SplitRegions {
            empty_input: keys.is_empty(),
            retry: SplitRetry::new(keys, retry_limit),
        })
    }
    /// Use the coordinator's actual controller and checker pending owner. Typed
    /// per-region admission/build failures are retained in the returned submissions.
    pub fn start_split_attempt(
        &self,
        request: &mut SplitRegions,
        now_ms: u64,
        replication: &RegionReplicationContext<'_, C>,
    ) -> Result<Option<SplitSubmissions>, HandlerError> {
        Ok(self.coordinator()?.get_region_splitter().start_attempt(
            &mut request.retry,
            now_ms,
            replication,
        ))
    }
}
