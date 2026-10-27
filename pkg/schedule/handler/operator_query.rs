// Copyright 2026 AsterSQL.
//! Domain management view over the coordinator's canonical operator owners.
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_operator::{
    ArchivedOperator, CancelReason, ControllerAdmission, OpHistory, OpKind, Operator,
    ack_removed_operators,
};
use std::{sync::Arc, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HandlerError {
    #[error("TiKV cluster not bootstrapped, please start TiKV first")]
    NotBootstrapped,
    #[error("operator not found")]
    OperatorNotFound,
}
impl HandlerError {
    pub fn code(self) -> &'static str {
        match self {
            Self::NotBootstrapped => "PD:cluster:ErrNotBootstrapped",
            Self::OperatorNotFound => "PD:operator:ErrOperatorNotFound",
        }
    }
}
/// The host supplies its current optional coordinator for each management action.
/// Clock inputs are explicit; wire conversion and wall-clock mapping belong to adapters.
pub struct Handler<'a, C> {
    coordinator: Option<&'a Coordinator<C>>,
}
impl<'a, C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static>
    Handler<'a, C>
{
    pub fn new(coordinator: Option<&'a Coordinator<C>>) -> Self {
        Self { coordinator }
    }
    fn coordinator(&self) -> Result<&'a Coordinator<C>, HandlerError> {
        self.coordinator.ok_or(HandlerError::NotBootstrapped)
    }
    pub fn get_operator_controller(&self) -> Result<&'a ControllerAdmission, HandlerError> {
        Ok(self.coordinator()?.get_operator_controller())
    }
    pub fn get_operator(&self, id: u64) -> Result<Arc<Operator>, HandlerError> {
        self.get_operator_controller()?
            .get_operator(id)
            .ok_or(HandlerError::OperatorNotFound)
    }
    pub fn get_operator_status(
        &self,
        id: u64,
        now: Instant,
    ) -> Result<ArchivedOperator, HandlerError> {
        let c = self.coordinator()?;
        c.get_operator_records()
            .get_status(c.get_operator_controller(), id, now)
            .ok_or(HandlerError::OperatorNotFound)
    }
    fn archive_removed(&self, removed: &[Arc<Operator>], now: Instant) -> Result<(), HandlerError> {
        let c = self.coordinator()?;
        ack_removed_operators(removed, c.get_cluster(), &**c.get_config());
        c.get_operator_records().bury_removed(removed, now);
        Ok(())
    }
    pub fn remove_operator(&self, id: u64, now: Instant) -> Result<(), HandlerError> {
        let op = self.get_operator(id)?;
        let removed = self
            .get_operator_controller()?
            .remove_operator(&op, CancelReason::AdminStop);
        self.archive_removed(&removed, now)
    }
    pub fn remove_operators(&self, now: Instant) -> Result<(), HandlerError> {
        let removed = self
            .get_operator_controller()?
            .remove_operators(CancelReason::AdminStop);
        self.archive_removed(&removed, now)
    }
    pub fn get_operators(&self) -> Result<Vec<Arc<Operator>>, HandlerError> {
        Ok(self.get_operator_controller()?.operators())
    }
    pub fn get_waiting_operators(&self) -> Result<Vec<Arc<Operator>>, HandlerError> {
        Ok(self.get_operator_controller()?.waiting_operators())
    }
    fn of_kind(&self, kind: OpKind) -> Result<Vec<Arc<Operator>>, HandlerError> {
        Ok(self
            .get_operators()?
            .into_iter()
            .filter(|op| op.kind().0 & kind.0 != 0)
            .collect())
    }
    pub fn get_admin_operators(&self) -> Result<Vec<Arc<Operator>>, HandlerError> {
        self.of_kind(OpKind::ADMIN)
    }
    pub fn get_leader_operators(&self) -> Result<Vec<Arc<Operator>>, HandlerError> {
        self.of_kind(OpKind::LEADER)
    }
    pub fn get_region_operators(&self) -> Result<Vec<Arc<Operator>>, HandlerError> {
        self.of_kind(OpKind::REGION)
    }
    /// Preserve Go concatenation, including duplicates and reuse after unknown kinds.
    pub fn get_operators_by_kinds(
        &self,
        kinds: &[&str],
    ) -> Result<Vec<Arc<Operator>>, HandlerError> {
        let mut results = Vec::new();
        let mut previous = Vec::new();
        for kind in kinds {
            previous = match *kind {
                "admin" => self.get_admin_operators()?,
                "leader" => self.get_leader_operators()?,
                "region" => self.get_region_operators()?,
                "waiting" => self.get_waiting_operators()?,
                _ => previous,
            };
            results.extend(previous.iter().cloned());
        }
        Ok(results)
    }
    pub fn get_history(&self, from: Instant, now: Instant) -> Result<Vec<OpHistory>, HandlerError> {
        Ok(self
            .coordinator()?
            .get_operator_records()
            .get_history(from, now))
    }
    /// Owned canonical records expose OpRecord via ArchivedOperator::record.
    pub fn get_records(
        &self,
        from: Instant,
        now: Instant,
    ) -> Result<Vec<ArchivedOperator>, HandlerError> {
        let records = self
            .coordinator()?
            .get_operator_records()
            .get_records(from, now);
        if records.is_empty() {
            Err(HandlerError::OperatorNotFound)
        } else {
            Ok(records)
        }
    }
}
