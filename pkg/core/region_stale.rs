// Copyright 2026 AsterSQL.
//! Read-only epoch and term validation against the current root index.
use crate::{RegionEpoch, RegionInfo, RegionMeta, RegionState};
use std::{fmt, sync::Arc};

/// A stale heartbeat retains both metadata values for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleRegionError {
    pub incoming: RegionMeta,
    pub current: RegionMeta,
}
impl fmt::Display for StaleRegionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "region is stale: region {:?} origin {:?}",
            self.incoming, self.current
        )
    }
}
impl std::error::Error for StaleRegionError {}

/// Metadata validation failures, independent of protocol formatting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegionValidationError {
    NotFound {
        request: RegionMeta,
    },
    InvalidEpoch {
        request: RegionEpoch,
        current: RegionEpoch,
    },
}
impl fmt::Display for RegionValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { request } => {
                write!(f, "region not found, request region: {request:?}")
            }
            Self::InvalidEpoch { request, current } => write!(
                f,
                "invalid region epoch, request: {request:?}, current: {current:?}"
            ),
        }
    }
}
impl std::error::Error for RegionValidationError {}

impl RegionInfo {
    /// Whether unsafe recovery recreated a region, excluding whole-cluster bootstrap.
    pub fn is_region_recreated(&self) -> bool {
        self.meta().epoch.unwrap_or_default()
            == (RegionEpoch {
                version: 1,
                conf_ver: 1,
            })
            && (!self.meta().key_range.start_key.is_empty()
                || !self.meta().key_range.end_key.is_empty())
    }
}

/// Checks overlaps first, then the same-ID origin. A zero incoming term is unknown.
pub fn check_region(
    region: &RegionInfo,
    origin: Option<&RegionInfo>,
    overlaps: &[Arc<RegionInfo>],
) -> Result<(), StaleRegionError> {
    let epoch = region.meta().epoch.unwrap_or_default();
    let stale = |current: &RegionInfo| StaleRegionError {
        incoming: region.meta().clone(),
        current: current.meta().clone(),
    };
    for item in overlaps {
        if epoch.version < item.meta().epoch.unwrap_or_default().version
            && !region.is_region_recreated()
        {
            return Err(stale(item));
        }
    }
    if let Some(origin) = origin {
        let old = origin.meta().epoch.unwrap_or_default();
        if ((region.term > 0 && region.term < origin.term)
            || epoch.version < old.version
            || epoch.conf_ver < old.conf_ver)
            && !region.is_region_recreated()
        {
            return Err(stale(origin));
        }
    }
    Ok(())
}
/// Precheck snapshot and validation result; snapshots are returned even on rejection.
pub type RegionPreCheck = (
    Option<Arc<RegionInfo>>,
    Vec<Arc<RegionInfo>>,
    Result<(), StaleRegionError>,
);
impl RegionState {
    /// Validates a heartbeat without changing either index.
    pub fn pre_check_put_region(&self, region: &RegionInfo) -> RegionPreCheck {
        let origin = self.get_region(region.meta().id);
        let overlaps = if origin
            .as_ref()
            .is_none_or(|old| old.meta().key_range != region.meta().key_range)
        {
            self.root.overlaps(region)
        } else {
            vec![]
        };
        let result = check_region(region, origin.as_deref(), &overlaps);
        (origin, overlaps, result)
    }
    /// Validates metadata against the region containing its start key, regardless of ID.
    pub fn valid_region(&self, region: &RegionMeta) -> Result<(), RegionValidationError> {
        let Some(current) = self.root.search(&region.key_range.start_key) else {
            return Err(RegionValidationError::NotFound {
                request: region.clone(),
            });
        };
        let request = region.epoch.unwrap_or_default();
        let current = current.meta().epoch.unwrap_or_default();
        if request.version < current.version || request.conf_ver < current.conf_ver {
            return Err(RegionValidationError::InvalidEpoch { request, current });
        }
        Ok(())
    }
}
