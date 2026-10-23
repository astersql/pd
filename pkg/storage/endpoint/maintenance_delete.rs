// Copyright 2026 AsterSQL.

use crate::{EndpointError, LoadState, MAINTENANCE_LOCK_NAME, MaintenanceTask, StorageEndpoint};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Mutation, RawTxn, RawTxnCmpType, RawTxnCondition, RawTxnOp};

impl StorageEndpoint {
    /// Load one task; absent and empty values are missing, malformed JSON is an error.
    pub async fn load_maintenance_task(
        &self,
        paths: &ClusterPaths,
        task_type: &str,
    ) -> Result<Option<MaintenanceTask>, EndpointError> {
        match self
            .load(paths.maintenance_task_path(task_type).as_bytes())
            .await?
        {
            LoadState::Missing | LoadState::Empty => Ok(None),
            LoadState::Value(bytes) => serde_json::from_str(&String::from_utf8_lossy(&bytes))
                .map(Some)
                .map_err(EndpointError::JsonUnmarshal),
        }
    }

    /// Deliver every raw suffix/value pair, including the maintenance lock, in pages of 100.
    pub async fn load_all_maintenance_tasks(
        &self,
        paths: &ClusterPaths,
        callback: impl FnMut(&[u8], &[u8]),
    ) -> Result<(), EndpointError> {
        self.load_rule_prefix(paths.maintenance_task_path_prefix().as_bytes(), callback)
            .await
    }

    /// Compare the lock with type/id and atomically delete both keys on a match.
    /// A conflict returns the first decodable task with the requested type from
    /// the transaction's bounded range snapshot, regardless of its key suffix.
    pub async fn try_delete_maintenance_task_atomic(
        &self,
        paths: &ClusterPaths,
        task_type: &str,
        task_id: &str,
    ) -> Result<(bool, Option<MaintenanceTask>), EndpointError> {
        let lock = paths
            .maintenance_task_path(MAINTENANCE_LOCK_NAME)
            .into_bytes();
        let prefix = paths.maintenance_task_path_prefix().into_bytes();
        let mut end = prefix.clone();
        end.push(0xff);
        let response = RawTxn::new(self.store.as_ref())
            .when(vec![RawTxnCondition {
                key: lock.clone(),
                cmp_type: RawTxnCmpType::Equal,
                value: format!("{task_type}/{task_id}").into_bytes(),
            }])
            .then_ops(vec![
                Mutation::Delete { key: lock.clone() }.into(),
                Mutation::Delete {
                    key: paths.maintenance_task_path(task_type).into_bytes(),
                }
                .into(),
            ])
            .else_ops(vec![RawTxnOp::GetRange {
                key: prefix,
                end,
                limit: 100,
            }])
            .commit()
            .await?;
        if response.succeeded {
            return Ok((true, None));
        }
        if let Some(items) = response.responses.first() {
            for pair in &items.key_value_pairs {
                if pair.key == lock {
                    continue;
                }
                if let Ok(task) =
                    serde_json::from_str::<MaintenanceTask>(&String::from_utf8_lossy(&pair.value))
                {
                    if task.task_type == task_type {
                        return Ok((false, Some(task)));
                    }
                }
            }
        }
        Ok((false, None))
    }
}
