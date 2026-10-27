// Copyright 2026 AsterSQL.
//! Range acceleration and replication observations over host-owned state.
use crate::{Handler, HandlerError};
use astersql_pd_core::RegionInfo;
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_filter::{RegionReplicationContext, is_region_replicated};
use astersql_pd_schedule_placement::{RegionFit, RuleManager};
use std::{cell::RefCell, sync::Arc};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegionAdminError {
    #[error(transparent)]
    Handler(#[from] HandlerError),
    #[error("{0}")]
    Hex(String),
    #[error("startKeys and endKeys should have the same length")]
    MismatchedRanges,
    #[error("invalid region limit: {0}")]
    InvalidLimit(String),
    #[error("placement rules feature is disabled")]
    PlacementDisabled,
}
fn decode(s: &str) -> Result<Vec<u8>, RegionAdminError> {
    crate::admin_parse::decode_hex(s).map_err(|e| match e {
        crate::CreationError::Scatter(message) => RegionAdminError::Hex(message),
        _ => unreachable!("hex decoder only produces hex errors"),
    })
}
/// Preserve Atoi's signed decimal, first default, upper cap and nonpositive values.
pub fn adjust_region_limit(input: &str, defaults: &[isize]) -> Result<isize, RegionAdminError> {
    let limit = if input.is_empty() {
        defaults.first().copied().unwrap_or(16)
    } else {
        let digits = input.strip_prefix(['+', '-']).unwrap_or(input);
        if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return Err(RegionAdminError::InvalidLimit(input.into()));
        }
        input
            .parse()
            .map_err(|_| RegionAdminError::InvalidLimit(input.into()))?
    };
    Ok(limit.min(10240))
}
impl<C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static> Handler<'_, C> {
    /// Parse the optional signed decimal scan limit, independent of bootstrap state.
    pub fn adjust_limit(&self, input: &str, defaults: &[isize]) -> Result<isize, RegionAdminError> {
        adjust_region_limit(input, defaults)
    }
    /// Decode before checking bootstrap, matching the hex management entry point.
    pub fn accelerate_regions_schedule_in_range(
        &self,
        start: &str,
        end: &str,
        limit: isize,
    ) -> Result<(), RegionAdminError> {
        let start = decode(start)?;
        let end = decode(end)?;
        self.accelerate_regions_schedule_in_ranges(&[start], &[end], limit)
    }
    /// The limit applies independently to each range; the canonical queue deduplicates IDs.
    pub fn accelerate_regions_schedule_in_ranges(
        &self,
        starts: &[Vec<u8>],
        ends: &[Vec<u8>],
        limit: isize,
    ) -> Result<(), RegionAdminError> {
        let c = self.coordinator()?;
        if starts.len() != ends.len() {
            return Err(RegionAdminError::MismatchedRanges);
        }
        let ids: Vec<_> = starts
            .iter()
            .zip(ends)
            .flat_map(|(start, end)| {
                c.get_cluster()
                    .regions
                    .scan_regions(start, end, limit)
                    .into_iter()
                    .map(|r| r.meta().id)
            })
            .collect();
        if !ids.is_empty() {
            c.get_checker_controller()
                .lock()
                .unwrap()
                .add_pending_processed_regions(false, &ids);
        }
        Ok(())
    }
    /// Observe actual fits and pending membership. A pending fully replicated region
    /// does not change the state; a pending incomplete region takes precedence.
    pub fn check_regions_replicated(
        &self,
        start: &str,
        end: &str,
        manager: &mut RuleManager,
        now: i128,
    ) -> Result<&'static str, RegionAdminError> {
        let start = decode(start)?;
        let end = decode(end)?;
        let c = self.coordinator()?;
        let stores = c.get_cluster().stores.get_stores();
        let context = RegionReplicationContext {
            conf: &**c.get_config(),
            stores: &stores,
            manager: RefCell::new(manager),
            now,
        };
        let mut state = "REPLICATED";
        for region in c.get_cluster().regions.scan_regions(&start, &end, -1) {
            if !is_region_replicated(&context, &region) {
                state = "INPROGRESS";
                if c.is_pending_region(region.meta().id) {
                    return Ok("PENDING");
                }
            }
        }
        Ok(state)
    }
    /// Fit using the host's canonical manager and current stores/configuration.
    pub fn check_region_placement_rule(
        &self,
        region: &RegionInfo,
        manager: &mut RuleManager,
        now: i128,
    ) -> Result<Arc<RegionFit>, RegionAdminError> {
        let c = self.coordinator()?;
        if !c.get_config().is_placement_rules_enabled() {
            return Err(RegionAdminError::PlacementDisabled);
        }
        Ok(manager.fit_region(
            &c.get_cluster().stores.get_stores(),
            region,
            &**c.get_config(),
            now,
        ))
    }
}
