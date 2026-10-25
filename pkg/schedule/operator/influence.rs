// Copyright 2026 AsterSQL.
//! Pending operator deltas and store-limit costs, independent of transports.
use std::{collections::HashMap, sync::RwLock};

use astersql_pd_core::is_empty_region;
use astersql_pd_core_constant::{ResourceKind, ScheduleKind, SchedulePolicy};
use astersql_pd_storelimit::{
    REGION_INFLUENCE, SMALL_REGION_INFLUENCE, SMALL_REGION_THRESHOLD, TYPE_NAME_VALUE, Type,
};

/// Signed resource and cost deltas for one store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreInfluence {
    pub region_size: i64,
    pub region_count: i64,
    pub leader_size: i64,
    pub leader_count: i64,
    pub witness_count: i64,
    pub step_cost: HashMap<Type, i64>,
}

impl StoreInfluence {
    /// Adds resource deltas and the four known cost types, as Go's type table does.
    pub fn add(&mut self, other: &Self) {
        self.region_size = self.region_size.wrapping_add(other.region_size);
        self.region_count = self.region_count.wrapping_add(other.region_count);
        self.leader_size = self.leader_size.wrapping_add(other.leader_size);
        self.leader_count = self.leader_count.wrapping_add(other.leader_count);
        self.witness_count = self.witness_count.wrapping_add(other.witness_count);
        for (_, typ) in TYPE_NAME_VALUE {
            self.add_step_cost(typ, other.get_step_cost(typ));
        }
    }

    /// Returns the delta selected by the resource and policy, including Go defaults.
    pub fn resource_property(&self, kind: ScheduleKind) -> i64 {
        match (kind.resource, kind.policy) {
            (ResourceKind::LEADER, SchedulePolicy::BY_COUNT) => self.leader_count,
            (ResourceKind::LEADER, SchedulePolicy::BY_SIZE) => self.leader_size,
            (ResourceKind::REGION, SchedulePolicy::BY_COUNT) => self.region_count,
            (ResourceKind::REGION, _) => self.region_size,
            (ResourceKind::WITNESS, _) => self.witness_count,
            _ => 0,
        }
    }

    /// Missing cost entries contribute zero.
    pub fn get_step_cost(&self, typ: Type) -> i64 {
        self.step_cost.get(&typ).copied().unwrap_or(0)
    }

    /// Accumulates a signed cost with Go-compatible integer wrapping.
    pub fn add_step_cost(&mut self, typ: Type, cost: i64) {
        let entry = self.step_cost.entry(typ).or_default();
        *entry = entry.wrapping_add(cost);
    }

    /// Charges a normal region above 20 MiB, a nonempty small region otherwise.
    ///
    /// # Panics
    /// Unsupported cost table indices are programmer errors, as in Go. Small
    /// nonempty regions support only add/remove peer; empty regions index no table.
    pub fn adjust_step_cost(&mut self, typ: Type, region_size: i64, region_keys: i64) {
        if region_size > SMALL_REGION_THRESHOLD {
            self.add_step_cost(typ, REGION_INFLUENCE[typ.as_i64() as usize]);
        } else if !is_empty_region(region_size, region_keys) {
            self.add_step_cost(typ, SMALL_REGION_INFLUENCE[typ.as_i64() as usize]);
        }
    }
}

/// Cluster influence with atomic updates and owned, deeply copied snapshots.
#[derive(Debug, Default)]
pub struct OpInfluence {
    stores: RwLock<HashMap<u64, StoreInfluence>>,
}

impl OpInfluence {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a missing zero store and returns an owned snapshot.
    pub fn get_store_influence(&self, id: u64) -> StoreInfluence {
        self.stores.write().unwrap().entry(id).or_default().clone()
    }

    /// Mutates one store atomically. The callback must not reenter this container.
    pub fn update_store(&self, id: u64, update: impl FnOnce(&mut StoreInfluence)) {
        update(self.stores.write().unwrap().entry(id).or_default());
    }

    /// Copies resource fields and cost maps under one read lock.
    pub fn snapshot(&self) -> HashMap<u64, StoreInfluence> {
        self.stores.read().unwrap().clone()
    }

    /// Merges a source snapshot, releasing its lock before locking this container.
    /// Self-add doubles the snapshot; reciprocal concurrent adds cannot deadlock.
    pub fn add(&self, other: &Self) {
        let copy = other.snapshot();
        let mut stores = self.stores.write().unwrap();
        for (id, influence) in copy {
            stores.entry(id).or_default().add(&influence);
        }
    }
}
