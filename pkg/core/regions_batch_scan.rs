// Copyright 2026 AsterSQL.
//! Multi-range scans, preserving Go's per-range validation and total limit.
use crate::{KeyRange, RegionInfo, RegionState};
use std::{fmt, sync::Arc};

/// Options corresponding to Go's WithLimit and WithOutputMustContainAllKeyRange.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatchScanRegionsOptions {
    /// Nonpositive values disable the limit.
    pub limit: isize,
    /// Fail on holes encountered before the per-range limit is reached.
    pub output_must_contain_all_key_range: bool,
}

/// A gap in a requested range. No partial batch is returned on failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionNotAdjacent {
    pub requested: KeyRange,
    pub missing: KeyRange,
    /// Last scanned range, or the synthetic initial range ending at the request start.
    pub previous: KeyRange,
    /// First range beyond an internal hole; absent for a trailing hole.
    pub next: Option<KeyRange>,
}
impl RegionNotAdjacent {
    /// Stable Go error identity.
    pub fn code(&self) -> &'static str {
        "ErrRegionNotAdjacent"
    }
}
impl fmt::Display for RegionNotAdjacent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = |v: &[u8]| v.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let r = &self.requested;
        let p = &self.previous;
        if let Some(n) = &self.next {
            write!(
                f,
                "[PD:region:ErrRegionNotAdjacent]key range[{}, {}) found a hole region between region[{}, {}) and region[{}, {})",
                hex(&r.start_key),
                hex(&r.end_key),
                hex(&p.start_key),
                hex(&p.end_key),
                hex(&n.start_key),
                hex(&n.end_key)
            )
        } else {
            write!(
                f,
                "[PD:region:ErrRegionNotAdjacent]key range[{}, {}) found a hole region in the last, the last scanned region is [{}, {}), [{}, {}) is missing",
                hex(&r.start_key),
                hex(&r.end_key),
                hex(&p.start_key),
                hex(&p.end_key),
                hex(&self.missing.start_key),
                hex(&self.missing.end_key)
            )
        }
    }
}
impl std::error::Error for RegionNotAdjacent {}

impl RegionState {
    /// Scans ordered, non-overlapping ranges. Adjacent input ranges are merged;
    /// callers retain their input. Shared boundary regions appear only once.
    /// Each range is validated with the full limit before the total is truncated.
    pub fn batch_scan_regions(
        &self,
        ranges: &[KeyRange],
        options: BatchScanRegionsOptions,
    ) -> Result<Vec<Arc<RegionInfo>>, RegionNotAdjacent> {
        let mut merged: Vec<KeyRange> = Vec::with_capacity(ranges.len());
        for range in ranges {
            if let Some(last) = merged.last_mut()
                && last.end_key == range.start_key
            {
                last.end_key.clone_from(&range.end_key);
                continue;
            }
            merged.push(range.clone());
        }
        let mut result: Vec<Arc<RegionInfo>> = Vec::new();
        for range in merged {
            let candidates = self.scan_regions(&range.start_key, &range.end_key, options.limit);
            if options.output_must_contain_all_key_range {
                let mut previous = KeyRange {
                    start_key: vec![],
                    end_key: range.start_key.clone(),
                };
                let mut last_end = range.start_key.as_slice();
                for region in &candidates {
                    let current = &region.meta().key_range;
                    if !last_end.is_empty()
                        && !current.start_key.is_empty()
                        && current.start_key.as_slice() > last_end
                    {
                        return Err(RegionNotAdjacent {
                            requested: range.clone(),
                            previous: previous.clone(),
                            next: Some(current.clone()),
                            missing: KeyRange {
                                start_key: last_end.to_vec(),
                                end_key: current.start_key.clone(),
                            },
                        });
                    }
                    last_end = &current.end_key;
                    previous = current.clone();
                }
                if !(options.limit > 0 && candidates.len() >= options.limit as usize)
                    && !range.end_key.is_empty()
                    && !last_end.is_empty()
                    && last_end < range.end_key.as_slice()
                {
                    return Err(RegionNotAdjacent {
                        requested: range.clone(),
                        previous,
                        next: None,
                        missing: KeyRange {
                            start_key: last_end.to_vec(),
                            end_key: range.end_key.clone(),
                        },
                    });
                }
            }
            let skip = usize::from(
                result
                    .last()
                    .zip(candidates.first())
                    .is_some_and(|(a, b)| a.meta().id == b.meta().id),
            );
            result.extend(candidates.into_iter().skip(skip));
            if options.limit > 0 && result.len() >= options.limit as usize {
                result.truncate(options.limit as usize);
                break;
            }
        }
        Ok(result)
    }
}
