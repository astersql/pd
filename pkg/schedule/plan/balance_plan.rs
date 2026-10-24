// Copyright 2026 AsterSQL.
//! Balance diagnostic resources; snapshots share immutable core handles.
use crate::Status;
use astersql_pd_core::{RegionInfo, StoreInfo};
use std::sync::Arc;

/// Explicit replacement for Go's dynamically typed resource argument.
#[derive(Clone)]
pub enum Resource {
    Store(Option<Arc<StoreInfo>>),
    Region(Option<Arc<RegionInfo>>),
}
/// Balance diagnostic snapshot. Steps 0/1/2 select source/region/target.
#[derive(Clone, Default)]
pub struct BalanceSchedulerPlan {
    pub source: Option<Arc<StoreInfo>>,
    pub region: Option<Arc<RegionInfo>>,
    pub target: Option<Arc<StoreInfo>>,
    pub status: Status,
    pub step: i64,
}
impl BalanceSchedulerPlan {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_step(&self) -> i64 {
        self.step
    }
    pub fn get_status(&self) -> &Status {
        &self.status
    }
    pub fn set_status(&mut self, status: Status) {
        self.status = status;
    }
    /// Wrong resource variants panic at known steps, as Go's type assertion does.
    pub fn set_resource(&mut self, resource: Resource) {
        match (self.step, resource) {
            (0, Resource::Store(v)) => self.source = v,
            (1, Resource::Region(v)) => self.region = v,
            (2, Resource::Store(v)) => self.target = v,
            (0..=2, _) => panic!("resource type does not match balance step"),
            _ => (),
        }
    }
    pub fn set_resource_with_step(&mut self, resource: Resource, step: i64) {
        self.step = step;
        self.set_resource(resource);
    }
    pub fn get_resource(&self, step: i64) -> u64 {
        if self.step < step {
            return 0;
        }
        match step {
            0 => self.source.as_ref().expect("source is absent").id(),
            1 => self.region.as_ref().expect("region is absent").meta().id,
            2 => self.target.as_ref().expect("target is absent").id(),
            _ => 0,
        }
    }
    /// Clone only completed steps, then apply options in caller order.
    pub fn clone_with(&self, options: &[crate::PlanOption]) -> Self {
        let mut plan = Self {
            source: if self.step > 0 {
                self.source.clone()
            } else {
                None
            },
            region: if self.step > 1 {
                self.region.clone()
            } else {
                None
            },
            target: if self.step > 2 {
                self.target.clone()
            } else {
                None
            },
            status: self.status.clone(),
            step: self.step,
        };
        for option in options {
            option.apply(&mut plan);
        }
        plan
    }
}
