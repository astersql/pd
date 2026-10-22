// Copyright 2026 AsterSQL.
//! Sequential list encoding with caller-controlled cancellation.
use crate::{RegionInfo, init_region};
use astersql_pd_core::RegionInfo as CoreRegion;
use std::fmt;
/// Context completion reason, supplied by the host without a runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionListCancellation {
    Cancelled,
    DeadlineExceeded,
}
/// List encoding failure. Cancellation never exposes partially encoded bytes.
#[derive(Debug)]
pub enum RegionListError {
    Cancelled(RegionListCancellation),
    Encoding(serde_json::Error),
}
impl fmt::Display for RegionListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled(RegionListCancellation::Cancelled) => f.write_str("context canceled"),
            Self::Cancelled(RegionListCancellation::DeadlineExceeded) => {
                f.write_str("context deadline exceeded")
            }
            Self::Encoding(err) => err.fmt(f),
        }
    }
}
impl std::error::Error for RegionListError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encoding(err) => Some(err),
            _ => None,
        }
    }
}
/// Encode count and ordered entries, retaining duplicates and Go's response reuse.
/// The host callback is checked before every entry, including absent entries.
/// Empty lists succeed without checking cancellation, matching the Go loop.
pub fn marshal_regions_info_json(
    regions: &[Option<&CoreRegion>],
    mut cancellation: impl FnMut() -> Option<RegionListCancellation>,
) -> Result<Vec<u8>, RegionListError> {
    let mut bytes = format!("{{\"count\":{},\"regions\":[", regions.len()).into_bytes();
    let mut response = RegionInfo::default();
    for (index, region) in regions.iter().enumerate() {
        if let Some(reason) = cancellation() {
            return Err(RegionListError::Cancelled(reason));
        }
        if index > 0 {
            bytes.push(b',');
        }
        init_region(*region, &mut response);
        serde_json::to_writer(&mut bytes, &response).map_err(RegionListError::Encoding)?;
    }
    bytes.extend_from_slice(b"]}");
    Ok(bytes)
}
