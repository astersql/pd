// Copyright 2026 AsterSQL.
//! Service registration, recovery markers and maintenance keys.
use crate::ClusterPaths;

impl ClusterPaths {
    /// Returns the service registry prefix, preserving the service name verbatim.
    pub fn service_path(&self, service_name: &str) -> String {
        format!("/ms/{}/{service_name}/registry/", self.cluster_id())
    }
    /// Returns the registration key, preserving the address verbatim.
    pub fn registry_path(&self, service_name: &str, service_addr: &str) -> String {
        format!("{}{service_addr}", self.service_path(service_name))
    }
    /// Returns the snapshot recovery marker key.
    pub fn recovering_mark_path(&self) -> String {
        format!(
            "/pd/{}/cluster/markers/snapshot-recovering",
            self.cluster_id()
        )
    }
    /// Returns the point-in-time restore mode marker key.
    pub fn pitr_restore_mode_mark_path(&self) -> String {
        format!(
            "/pd/{}/cluster/markers/pitr-restore-mode",
            self.cluster_id()
        )
    }
    /// Returns the maintenance task key, preserving the task type verbatim.
    pub fn maintenance_task_path(&self, task_type: &str) -> String {
        format!("/pd/{}/maintenance/{task_type}", self.cluster_id())
    }
    /// Returns the maintenance prefix, including its trailing slash.
    pub fn maintenance_task_path_prefix(&self) -> String {
        self.maintenance_task_path("")
    }
}
