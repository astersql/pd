// Copyright 2026 AsterSQL.

use std::fmt;

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Mutation, RawTxn, RawTxnCmpType, RawTxnCondition, RawTxnOp};
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{EndpointError, StorageEndpoint};

/// Global maintenance lock shared by all task types.
pub const MAINTENANCE_LOCK_NAME: &str = "maintenance_lock";

/// Persisted maintenance task; field order and names match Go's JSON schema.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MaintenanceTask {
    #[serde(rename = "type")]
    pub task_type: String,
    pub id: String,
    pub start_timestamp: i64,
    pub description: String,
}

impl<'de> Deserialize<'de> for MaintenanceTask {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TaskVisitor;
        impl<'de> de::Visitor<'de> for TaskVisitor {
            type Value = MaintenanceTask;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a maintenance task object or null")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(MaintenanceTask::default())
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut task = MaintenanceTask::default();
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("type") {
                        if let Some(value) = map.next_value::<Option<String>>()? {
                            task.task_type = value;
                        }
                    } else if key.eq_ignore_ascii_case("id") {
                        if let Some(value) = map.next_value::<Option<String>>()? {
                            task.id = value;
                        }
                    } else if key.eq_ignore_ascii_case("start_timestamp") {
                        if let Some(value) = map.next_value::<Option<i64>>()? {
                            task.start_timestamp = value;
                        }
                    } else if key.eq_ignore_ascii_case("description") {
                        if let Some(value) = map.next_value::<Option<String>>()? {
                            task.description = value;
                        }
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(task)
            }
        }
        deserializer.deserialize_any(TaskVisitor)
    }
}

impl MaintenanceTask {
    pub(crate) fn go_json(&self) -> Result<Vec<u8>, EndpointError> {
        Ok(serde_json::to_string(self)
            .map_err(EndpointError::JsonMarshal)?
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029")
            .into_bytes())
    }
}

/// Start failures distinguish backend/encoding errors from inconsistent stored tasks.
#[derive(Debug, thiserror::Error)]
pub enum MaintenanceStartError {
    #[error(transparent)]
    Endpoint(#[from] EndpointError),
    #[error("multiple maintenance tasks running simultaneously: found {0} tasks")]
    MultipleTasks(usize),
}

impl StorageEndpoint {
    /// Atomically acquire the global lock and save the task. A conflict returns the
    /// current holder from the same transaction snapshot, skipping malformed JSON.
    pub async fn try_start_maintenance_task_atomic(
        &self,
        paths: &ClusterPaths,
        task: &MaintenanceTask,
    ) -> Result<(bool, Option<MaintenanceTask>), MaintenanceStartError> {
        let lock_key = paths
            .maintenance_task_path(MAINTENANCE_LOCK_NAME)
            .into_bytes();
        let task_key = paths.maintenance_task_path(&task.task_type).into_bytes();
        let prefix = paths.maintenance_task_path_prefix().into_bytes();
        let mut end = prefix.clone();
        end.push(0xff);
        let response = RawTxn::new(self.store.as_ref())
            .when(vec![RawTxnCondition {
                key: lock_key.clone(),
                cmp_type: RawTxnCmpType::NotExists,
                value: Vec::new(),
            }])
            .then_ops(vec![
                Mutation::put(
                    &lock_key,
                    format!("{}/{}", task.task_type, task.id).as_bytes(),
                )
                .into(),
                Mutation::put(&task_key, &task.go_json()?).into(),
            ])
            .else_ops(vec![RawTxnOp::GetRange {
                key: prefix,
                end,
                limit: 100,
            }])
            .commit()
            .await
            .map_err(EndpointError::from)?;
        if response.succeeded {
            return Ok((true, None));
        }
        let mut existing = None;
        let mut count = 0;
        if let Some(items) = response.responses.first() {
            for pair in &items.key_value_pairs {
                if pair.key == lock_key {
                    continue;
                }
                // Go encoding/json replaces invalid UTF-8 in JSON strings.
                if let Ok(task) =
                    serde_json::from_str::<MaintenanceTask>(&String::from_utf8_lossy(&pair.value))
                {
                    existing = Some(task);
                    count += 1;
                }
            }
        }
        if count > 1 {
            return Err(MaintenanceStartError::MultipleTasks(count));
        }
        Ok((false, existing))
    }
}
