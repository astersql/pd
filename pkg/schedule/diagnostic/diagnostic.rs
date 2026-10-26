// Copyright 2026 AsterSQL.
use astersql_pd_errs::{TypedError, catalog::*};
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_schedulers::{
    DISABLED, DiagnosticRecorder, DiagnosticResult, SchedulerController,
};
use std::collections::BTreeMap;

/// Borrowed query view. The host serializes controller/config/recorder access;
/// recorders here are the same objects populated by its scheduling path.
pub struct Manager<'a, C> {
    controller: &'a SchedulerController,
    config: &'a C,
    recorders: &'a BTreeMap<String, DiagnosticRecorder>,
}
impl<'a, C: SchedulerConfigProvider> Manager<'a, C> {
    /// Creates a view over the host's synchronized scheduling state.
    pub fn new(
        controller: &'a SchedulerController,
        config: &'a C,
        recorders: &'a BTreeMap<String, DiagnosticRecorder>,
    ) -> Self {
        Self {
            controller,
            config,
            recorders,
        }
    }
    /// Queries using host-supplied Unix seconds, including recorder aggregation.
    pub fn get_diagnostic_result(
        &self,
        name: &str,
        timestamp: u64,
    ) -> Result<DiagnosticResult, TypedError> {
        if !self.config.is_diagnostic_allowed() {
            return Err(ERR_DIAGNOSTIC_DISABLED.instantiate([] as [&str; 0]));
        }
        let scheduler = self.controller.get(name);
        if scheduler.is_none_or(|s| {
            self.config
                .is_scheduler_disabled(&s.scheduler.scheduler_type())
        }) {
            return Ok(DiagnosticResult {
                name: name.into(),
                status: DISABLED.into(),
                summary: String::new(),
                timestamp,
                store_status: BTreeMap::new(),
            });
        }
        let recorder = self
            .recorders
            .get(name)
            .ok_or_else(|| ERR_SCHEDULER_UNDIAGNOSABLE.instantiate([name]))?;
        recorder
            .get_last_result(timestamp)
            .ok_or_else(|| ERR_NO_DIAGNOSTIC_RESULT.instantiate([name]))
    }
}
