// Copyright 2026 AsterSQL.
//! GC key schema from Go gc_states.go; values and GC algorithms are separate.
use crate::ClusterPaths;

/// Sentinel for unified GC, distinct from the default keyspace (zero).
pub const NULL_KEYSPACE_ID: u32 = u32::MAX;
/// Service ID used by the GC worker.
pub const GC_WORKER_SERVICE_SAFE_POINT_ID: &str = "gc_worker";
/// Service ID used by native BR.
pub const NATIVE_BR_SERVICE_SAFE_POINT_ID: &str = "native_br";

impl ClusterPaths {
    /// Returns the GC state revision key.
    pub fn gc_state_revision_path(&self) -> String {
        format!("/pd/{}/gc/gc_state_revision", self.cluster_id())
    }
    /// Returns the unified or keyspace GC safe point key.
    pub fn gc_safe_point_path(&self, keyspace_id: u32) -> String {
        if keyspace_id == NULL_KEYSPACE_ID {
            format!("/pd/{}/gc/safe_point", self.cluster_id())
        } else {
            self.gc_safe_point_v2_path(keyspace_id)
        }
    }
    /// Returns the TiDB transaction safe point key, independent of cluster ID.
    pub fn txn_safe_point_path(&self, keyspace_id: u32) -> String {
        if keyspace_id == NULL_KEYSPACE_ID {
            "/tidb/store/gcworker/saved_safe_point".into()
        } else {
            format!("/keyspaces/tidb/{keyspace_id}/tidb/store/gcworker/saved_safe_point")
        }
    }
    /// Returns the barrier prefix including its trailing slash.
    pub fn gc_barrier_prefix(&self, keyspace_id: u32) -> String {
        self.gc_barrier_path(keyspace_id, "")
    }
    /// Returns a barrier key, preserving the barrier ID verbatim.
    pub fn gc_barrier_path(&self, keyspace_id: u32, barrier_id: &str) -> String {
        if keyspace_id == NULL_KEYSPACE_ID {
            format!(
                "/pd/{}/gc/safe_point/service/{barrier_id}",
                self.cluster_id()
            )
        } else {
            self.service_safe_point_v2_path(keyspace_id, barrier_id)
        }
    }
    /// Returns the global barrier prefix including its trailing slash.
    pub fn global_gc_barrier_prefix(&self) -> String {
        self.global_gc_barrier_path("")
    }
    /// Returns a global barrier key, preserving the ID verbatim.
    pub fn global_gc_barrier_path(&self, barrier_id: &str) -> String {
        format!(
            "/pd/{}/gc/global_gc_barriers/{barrier_id}",
            self.cluster_id()
        )
    }
    /// Returns the legacy service prefix shared with unified barriers.
    pub fn service_gc_safe_point_prefix(&self) -> String {
        self.gc_barrier_prefix(NULL_KEYSPACE_ID)
    }
    /// Returns a legacy service key shared with unified barriers.
    pub fn service_gc_safe_point_path(&self, service_id: &str) -> String {
        self.gc_barrier_path(NULL_KEYSPACE_ID, service_id)
    }
    /// Returns the TiDB min-start-TS prefix including its trailing slash.
    pub fn compatible_tidb_min_start_ts_prefix(&self, keyspace_id: u32) -> String {
        if keyspace_id == NULL_KEYSPACE_ID {
            "/tidb/server/minstartts/".into()
        } else {
            format!("/keyspaces/tidb/{keyspace_id}/tidb/server/minstartts/")
        }
    }
    /// Returns the v2 GC prefix including its trailing slash.
    pub fn gc_safe_point_v2_prefix(&self) -> String {
        format!("/pd/{}/keyspaces/gc_safe_point/", self.cluster_id())
    }
    /// Returns a v2 key even for the null sentinel, without unified branching.
    pub fn gc_safe_point_v2_path(&self, keyspace_id: u32) -> String {
        format!("{}{keyspace_id:08}", self.gc_safe_point_v2_prefix())
    }
    /// Returns a v2 service key even for the null sentinel.
    pub fn service_safe_point_v2_path(&self, keyspace_id: u32, service_id: &str) -> String {
        format!(
            "/pd/{}/keyspaces/service_safe_point/{keyspace_id:08}/{service_id}",
            self.cluster_id()
        )
    }
    /// Returns the v2 service prefix including its trailing slash.
    pub fn service_safe_point_v2_prefix(&self, keyspace_id: u32) -> String {
        self.service_safe_point_v2_path(keyspace_id, "")
    }
}
