// Copyright 2026 AsterSQL.
//! Management queries borrow the coordinator's live controller and checker state.
use crate::{Handler, HandlerError};
use astersql_pd_errs::{TypedError, catalog::*};
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_schedulers::{ControlError, DiagnosticResult, SchedulerController};

#[derive(Debug, thiserror::Error)]
pub enum SchedulerAdminError {
    #[error(transparent)]
    Handler(#[from] HandlerError),
    #[error(transparent)]
    Control(#[from] ControlError),
    #[error(transparent)]
    Domain(#[from] TypedError),
}
impl SchedulerAdminError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Handler(e) => e.code(),
            Self::Domain(e) => e.code(),
            Self::Control(ControlError::NotBootstrapped) => "PD:cluster:ErrNotBootstrapped",
            Self::Control(ControlError::NotFound(_)) => "PD:scheduler:ErrSchedulerNotFound",
            Self::Control(_) => "PD:scheduler:ErrSchedulerConfig",
        }
    }
}
/// Wall-clock seconds, mapped to wire timestamps by the transport adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulerPausedPeriod {
    pub name: String,
    pub paused_at: i64,
    pub resume_at: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchedulerQuery {
    Names(Vec<String>),
    PausedPeriods(Vec<SchedulerPausedPeriod>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckerStatus {
    pub paused: bool,
}
impl<C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static> Handler<'_, C> {
    pub fn get_schedulers_controller(&self) -> Result<&SchedulerController, SchedulerAdminError> {
        Ok(self.coordinator()?.get_schedulers_controller())
    }
    pub fn get_scheduler_names(&self) -> Result<Vec<String>, SchedulerAdminError> {
        Ok(self.get_schedulers_controller()?.names())
    }
    /// Unknown status follows Go's default branch: return enabled schedulers.
    pub fn get_scheduler_by_status(
        &self,
        status: &str,
        need_timestamps: bool,
        now: i64,
    ) -> Result<SchedulerQuery, SchedulerAdminError> {
        let controller = self.get_schedulers_controller()?;
        let mut names = Vec::new();
        let mut periods = Vec::new();
        for name in controller.names() {
            if status == "paused" {
                if controller.is_paused(&name, now)? {
                    if need_timestamps {
                        let (paused_at, resume_at) = controller.delay(&name, now)?;
                        periods.push(SchedulerPausedPeriod {
                            name,
                            paused_at,
                            resume_at,
                        });
                    } else {
                        names.push(name);
                    }
                }
            } else if controller.is_disabled(&name)? == (status == "disabled") {
                names.push(name);
            }
        }
        Ok(if status == "paused" && need_timestamps {
            SchedulerQuery::PausedPeriods(periods)
        } else {
            SchedulerQuery::Names(names)
        })
    }
    pub fn get_checker_status(
        &self,
        name: &str,
        now: i64,
    ) -> Result<CheckerStatus, SchedulerAdminError> {
        let checkers = self.coordinator()?.get_checker_controller().lock().unwrap();
        let pause = checkers
            .pause_controller(name)
            .ok_or_else(|| ERR_CHECKER_NOT_FOUND.instantiate([] as [&str; 0]))?;
        Ok(CheckerStatus {
            paused: pause.is_paused_at(now),
        })
    }
    pub fn pause_or_resume_checker(
        &self,
        name: &str,
        ttl: i64,
        now: i64,
    ) -> Result<(), SchedulerAdminError> {
        let checkers = self.coordinator()?.get_checker_controller().lock().unwrap();
        let pause = checkers
            .pause_controller(name)
            .ok_or_else(|| ERR_CHECKER_NOT_FOUND.instantiate([] as [&str; 0]))?;
        pause.pause_or_resume_at(ttl, now);
        Ok(())
    }
    /// Validate diagnosability before bootstrap, preserving Go error precedence.
    pub fn get_diagnostic_result(
        &self,
        name: &str,
        timestamp: u64,
    ) -> Result<DiagnosticResult, SchedulerAdminError> {
        if !matches!(
            name,
            "balance-leader-scheduler" | "balance-region-scheduler"
        ) {
            return Err(ERR_SCHEDULER_UNDIAGNOSABLE.instantiate([name]).into());
        }
        Ok(self
            .coordinator()?
            .get_diagnostic_manager()
            .get_diagnostic_result(name, timestamp)?)
    }
}
/// The host serializes scheduler mutations using its exclusive coordinator borrow.
pub fn pause_or_resume_scheduler<
    C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static,
>(
    coordinator: Option<&mut Coordinator<C>>,
    name: &str,
    ttl: i64,
    now: i64,
) -> Result<(), SchedulerAdminError> {
    coordinator
        .ok_or(HandlerError::NotBootstrapped)?
        .get_schedulers_controller_mut()
        .pause_or_resume(name, ttl, now)?;
    Ok(())
}
