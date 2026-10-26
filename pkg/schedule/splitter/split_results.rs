// Copyright 2026 AsterSQL.
//! Split admission and observation over canonical cluster snapshots.
use crate::RegionGroupKeys;
use astersql_pd_core::{BasicCluster, RegionsInfo};
use astersql_pd_schedule_hbstream::SplitPolicy;
use astersql_pd_schedule_operator::{
    AdmissionError, AdmissionOutcome, BuilderError, ControllerAdmission, OpKind,
    create_split_region_operator,
};
use std::{collections::BTreeMap, fmt, sync::Arc};

/// Preserve factory and controller rejection reasons for the host.
#[derive(Debug)]
pub enum SplitExecutionError {
    Build(BuilderError),
    Admission(AdmissionError),
}
impl fmt::Display for SplitExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(e) => e.fmt(f),
            Self::Admission(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for SplitExecutionError {}
/// Admit a real USEKEY split operation. The host dispatches the returned outcome
/// through its existing controller; admission alone proves no split result.
pub fn split_region_by_keys(
    cluster: &BasicCluster,
    controller: &ControllerAdmission,
    group: &RegionGroupKeys,
) -> Result<AdmissionOutcome, SplitExecutionError> {
    let op = create_split_region_operator(
        "region-splitter",
        &group.region,
        OpKind::SPLIT,
        SplitPolicy::UseKey,
        group.keys.clone(),
    )
    .map_err(SplitExecutionError::Build)?;
    controller
        .add_operators(cluster, &[Arc::new(op)])
        .map_err(SplitExecutionError::Admission)
}
/// Region ID to observed start key, accumulated across scans within one attempt.
#[derive(Debug, Default)]
pub struct SplitKeyResults {
    regions: BTreeMap<u64, Vec<u8>>,
}
impl SplitKeyResults {
    pub fn regions(&self) -> &BTreeMap<u64, Vec<u8>> {
        &self.regions
    }
    /// Preserve input order and duplicates; matching starts consume every copy.
    pub fn unprocessed_keys(&self, keys: &[Vec<u8>]) -> Vec<Vec<u8>> {
        keys.iter()
            .filter(|key| !self.regions.values().any(|start| start == *key))
            .cloned()
            .collect()
    }
}
/// The immutable original range and keys plus this attempt's completion flag.
#[derive(Debug)]
pub struct SplitGroupProgress {
    pub group: RegionGroupKeys,
    finished: bool,
}
impl SplitGroupProgress {
    pub fn new(group: RegionGroupKeys) -> Self {
        Self {
            group,
            finished: false,
        }
    }
    pub fn finished(&self) -> bool {
        self.finished
    }
    /// Poll the current canonical index, retaining partial observations. The host
    /// owns watch cadence, timeout and cancellation. Finished groups need no poll.
    pub fn scan_regions_by_key_range(
        &mut self,
        regions: &RegionsInfo,
        results: &mut SplitKeyResults,
    ) {
        if self.finished {
            return;
        }
        let range = &self.group.region.meta().key_range;
        let mut created = BTreeMap::new();
        for region in regions.scan_regions(&range.start_key, &range.end_key, -1) {
            let start = &region.meta().key_range.start_key;
            if self.group.keys.iter().any(|key| key == start) {
                created.insert(region.meta().id, start.clone());
            }
        }
        if created.len() >= self.group.keys.len() {
            self.finished = true;
        }
        results.regions.extend(created);
    }
}

/// One submission pass and its observations. The host dispatches each admitted
/// outcome, then polls with current snapshots until finished or its deadline.
/// Rejected groups are never scanned; all unobserved input keys remain retryable.
#[derive(Debug, Default)]
pub struct SplitAttempt {
    groups: BTreeMap<u64, SplitGroupProgress>,
    results: SplitKeyResults,
}
/// Admission effects belong to the host controller's existing dispatch path.
pub struct SplitSubmissions {
    pub admitted: Vec<AdmissionOutcome>,
    pub rejected: BTreeMap<u64, SplitExecutionError>,
}
impl SplitAttempt {
    pub fn submit(
        cluster: &BasicCluster,
        controller: &ControllerAdmission,
        groups: BTreeMap<u64, RegionGroupKeys>,
    ) -> (Self, SplitSubmissions) {
        let mut attempt = Self::default();
        let mut submissions = SplitSubmissions {
            admitted: Vec::new(),
            rejected: BTreeMap::new(),
        };
        for (id, group) in groups {
            match split_region_by_keys(cluster, controller, &group) {
                Ok(outcome) => {
                    submissions.admitted.push(outcome);
                    attempt.groups.insert(id, SplitGroupProgress::new(group));
                }
                Err(error) => {
                    submissions.rejected.insert(id, error);
                }
            }
        }
        (attempt, submissions)
    }
    pub fn scan(&mut self, regions: &RegionsInfo) {
        for group in self.groups.values_mut() {
            group.scan_regions_by_key_range(regions, &mut self.results);
        }
    }
    pub fn finished(&self) -> bool {
        self.groups.values().all(SplitGroupProgress::finished)
    }
    pub fn results(&self) -> &SplitKeyResults {
        &self.results
    }
}
