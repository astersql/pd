// Copyright 2026 AsterSQL.
//! Ordered clone options and successful-first collection.
use crate::{BalanceSchedulerPlan, Resource, Status};

/// Operations applied sequentially to a fresh diagnostic clone.
#[derive(Clone)]
pub enum PlanOption {
    SetStatus(Status),
    SetResource(Resource),
    SetResourceWithStep(Resource, i64),
}
impl PlanOption {
    pub fn apply(&self, plan: &mut BalanceSchedulerPlan) {
        match self {
            Self::SetStatus(v) => plan.set_status(v.clone()),
            Self::SetResource(v) => plan.set_resource(v.clone()),
            Self::SetResourceWithStep(v, step) => plan.set_resource_with_step(v.clone(), *step),
        }
    }
}
/// Owns its base snapshot and keeps insertion order within each status group.
pub struct Collector {
    pub base_plan: BalanceSchedulerPlan,
    schedulable: Vec<BalanceSchedulerPlan>,
    unschedulable: Vec<BalanceSchedulerPlan>,
}
impl Collector {
    pub fn new(base_plan: BalanceSchedulerPlan) -> Self {
        Self {
            base_plan,
            schedulable: Vec::new(),
            unschedulable: Vec::new(),
        }
    }
    pub fn collect(&mut self, options: &[PlanOption]) {
        let plan = self.base_plan.clone_with(options);
        if plan.status.is_ok() {
            self.schedulable.push(plan);
        } else {
            self.unschedulable.push(plan);
        }
    }
    pub fn get_plans(&self) -> Vec<BalanceSchedulerPlan> {
        self.schedulable
            .iter()
            .chain(&self.unschedulable)
            .cloned()
            .collect()
    }
}
