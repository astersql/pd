// Copyright 2026 AsterSQL.
//! Bounded diagnostic history over canonical balance plans, without command submission.
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_operator::Operator;
use astersql_pd_schedule_plan::{Status, balance_plan_summary};
use astersql_pd_schedule_types::{
    BALANCE_LEADER_SCHEDULER, BALANCE_REGION_SCHEDULER, CheckerSchedulerType,
};
use std::{
    any::Any,
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

pub const DISABLED: &str = "disabled";
pub const PAUSED: &str = "paused";
pub const HALTED: &str = "halted";
pub const SCHEDULING: &str = "scheduling";
pub const PENDING: &str = "pending";
pub const NORMAL: &str = "normal";
const MAX_RESULTS: usize = 10;

/// Public response omits internal per-store observations, as in Go's JSON boundary.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct DiagnosticResult {
    pub name: String,
    pub status: String,
    pub summary: String,
    pub timestamp: u64,
    #[serde(skip)]
    pub store_status: BTreeMap<u64, Status>,
}
/// Owned history for one diagnosable scheduler. Hosts synchronize shared access.
/// Configuration and unix seconds are supplied at each call, so neither can go stale.
pub struct DiagnosticRecorder {
    scheduler_type: CheckerSchedulerType,
    results: VecDeque<DiagnosticResult>,
}
impl DiagnosticRecorder {
    /// Only leader and region balancing have a supported summary function.
    pub fn new(scheduler_type: CheckerSchedulerType) -> Option<Self> {
        if scheduler_type != BALANCE_LEADER_SCHEDULER && scheduler_type != BALANCE_REGION_SCHEDULER
        {
            return None;
        }
        Some(Self {
            scheduler_type,
            results: VecDeque::with_capacity(MAX_RESULTS),
        })
    }
    pub fn is_allowed(&self, config: &impl SchedulerConfigProvider) -> bool {
        config.is_diagnostic_allowed()
    }
    pub fn len(&self) -> usize {
        self.results.len()
    }
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }
    fn result(&self, status: &str, timestamp: u64) -> DiagnosticResult {
        DiagnosticResult {
            name: self.scheduler_type.to_string(),
            status: status.into(),
            summary: String::new(),
            timestamp,
            store_status: BTreeMap::new(),
        }
    }
    fn push(&mut self, result: DiagnosticResult) {
        if self.results.len() == MAX_RESULTS {
            self.results.pop_back();
        }
        self.results.push_front(result);
    }
    pub fn set_result_from_status(&mut self, status: &str, timestamp: u64) {
        self.push(self.result(status, timestamp));
    }
    /// Nonempty operators take precedence over all plans, including invalid plans.
    /// Go deliberately discards summary errors: malformed types yield pending/empty.
    /// The canonical summary retains its existing panic contract for missing resources.
    pub fn analyze(
        &self,
        operators: &[Arc<Operator>],
        plans: &[&dyn Any],
        timestamp: u64,
    ) -> DiagnosticResult {
        let mut result = self.result(SCHEDULING, timestamp);
        if !operators.is_empty() {
            return result;
        }
        result.status = PENDING.into();
        if let Ok((statuses, normal)) = balance_plan_summary(plans) {
            result.store_status = statuses;
            if normal {
                result.status = NORMAL.into();
            }
        }
        result
    }
    pub fn set_result_from_plans(
        &mut self,
        operators: &[Arc<Operator>],
        plans: &[&dyn Any],
        timestamp: u64,
    ) {
        let result = self.analyze(operators, plans, timestamp);
        self.push(result);
    }
    /// Aggregate only the newest uninterrupted status run, capped at ten records.
    /// Three segments receive relative weights 3/2/1; extra items go to earlier
    /// segments. Integer units avoid floating-point tie drift. Equal totals choose
    /// the lower (code, reason), refining Go's unspecified map traversal order.
    pub fn get_last_result(&self, timestamp: u64) -> Option<DiagnosticResult> {
        let first = self.results.front()?;
        let mut out = self.result(&first.status, timestamp);
        if first.status != PENDING && first.status != NORMAL {
            return Some(out);
        }
        let items: Vec<_> = self
            .results
            .iter()
            .take_while(|r| r.status == first.status)
            .collect();
        let n = items.len();
        let first_end = n / 3 + usize::from(n % 3 > 0);
        let second_end = first_end + n / 3 + usize::from(n % 3 > 1);
        let mut stores: BTreeMap<u64, BTreeMap<(i64, String), usize>> = BTreeMap::new();
        for (i, item) in items.iter().enumerate() {
            let weight = if i < first_end {
                3
            } else if i < second_end {
                2
            } else {
                1
            };
            for (id, status) in &item.store_status {
                *stores
                    .entry(*id)
                    .or_default()
                    .entry((status.status_code.0, status.detailed_reason.clone()))
                    .or_default() += weight;
            }
        }
        let mut counts = BTreeMap::new();
        for statuses in stores.values() {
            let mut best = None;
            let mut max = 0;
            for (status, weight) in statuses {
                if *weight > max {
                    best = Some(status);
                    max = *weight;
                }
            }
            if let Some(status) = best {
                *counts.entry(status.clone()).or_insert(0usize) += 1;
            }
        }
        for ((code, reason), count) in counts {
            let status = Status::new(astersql_pd_schedule_plan::StatusCode(code), &[&reason]);
            out.summary
                .push_str(&format!("{count} store(s) {status}; "));
        }
        if out.summary.is_empty() && out.status == PENDING {
            out.summary = format!("{} reach limit", self.scheduler_type);
        }
        Some(out)
    }
}
