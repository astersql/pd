// Copyright 2026 AsterSQL.
use crate::{Controller, RecoveryError, Stage};
impl Controller {
    /// The host supplies wall-clock rendering; the domain owns the exact deadline and strict comparison.
    pub fn check_timeout(
        &self,
        now: i128,
        render: impl FnOnce(i128) -> String,
    ) -> Result<(), RecoveryError> {
        if self.stage.is_running() && now > self.timeout_unix_nanos {
            return Err(RecoveryError::Backend(format!(
                "Exceeds timeout {}",
                render(self.timeout_unix_nanos)
            )));
        }
        Ok(())
    }
    /// Retain the earliest error and allow one bounded attempt to exit force leader.
    /// True means terminal failure: callers must stop dispatching immediately.
    pub fn handle_error(&mut self, error: RecoveryError, now: i128) -> bool {
        if self.recovery_error.is_none() {
            self.recovery_error = Some(error);
        }
        if self.stage == Stage::ExitForceLeader {
            self.change_stage(Stage::Failed, now);
            return true;
        }
        self.report_requests.clear();
        self.plans.clear();
        self.timeout_unix_nanos =
            now + i128::from(self.plan_execution_timeout_nanos.wrapping_mul(2));
        self.change_stage(Stage::ExitForceLeader, now);
        false
    }
    pub fn abort(&mut self, now: i128) -> Result<(), RecoveryError> {
        if !self.stage.is_running() {
            return Err(RecoveryError::Invalid("no ongoing unsafe recovery".into()));
        }
        if self.stage != Stage::ExitForceLeader {
            self.handle_error(RecoveryError::Backend("aborted by operator".into()), now);
        }
        Ok(())
    }
}
