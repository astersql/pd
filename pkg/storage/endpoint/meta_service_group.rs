// Copyright 2026 AsterSQL.

use std::{collections::BTreeMap, fmt};

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::CallbackTxn;
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{EndpointError, StorageEndpoint};

/// Persisted meta-service group status; missing state is disabled with no assignments.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MetaServiceGroupStatus {
    pub assignment_count: i64,
    pub enabled: bool,
}

impl<'de> Deserialize<'de> for MetaServiceGroupStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StatusVisitor;
        impl<'de> de::Visitor<'de> for StatusVisitor {
            type Value = MetaServiceGroupStatus;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a meta-service group status object or null")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(MetaServiceGroupStatus::default())
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut status = MetaServiceGroupStatus::default();
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("assignment_count") {
                        if let Some(count) = map.next_value::<Option<i64>>()? {
                            status.assignment_count = count;
                        }
                    } else if key.eq_ignore_ascii_case("enabled") {
                        if let Some(enabled) = map.next_value::<Option<bool>>()? {
                            status.enabled = enabled;
                        }
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(status)
            }
        }
        deserializer.deserialize_any(StatusVisitor)
    }
}

impl StorageEndpoint {
    /// Buffer JSON status (or Go's nil pointer as null) in the caller transaction.
    pub fn save_meta_service_group_status(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: &str,
        status: Option<&MetaServiceGroupStatus>,
    ) -> Result<(), EndpointError> {
        let bytes = serde_json::to_vec(&status).map_err(EndpointError::JsonMarshal)?;
        Ok(txn.save(paths.meta_service_group_status_path(id).as_bytes(), &bytes)?)
    }

    /// Load only requested IDs, ignoring addresses; absent and empty values are zero.
    /// Any read or decoding error discards the entire result.
    pub async fn load_meta_service_group_status(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        ids: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, MetaServiceGroupStatus>, EndpointError> {
        let mut statuses = BTreeMap::new();
        for id in ids.keys() {
            let bytes = txn
                .load(paths.meta_service_group_status_path(id).as_bytes())
                .await?;
            let status = if bytes.is_empty() {
                MetaServiceGroupStatus::default()
            } else {
                serde_json::from_slice(&bytes).map_err(EndpointError::JsonUnmarshal)?
            };
            statuses.insert(id.clone(), status);
        }
        Ok(statuses)
    }

    /// Buffer status removal so a later re-add starts with zero status.
    pub fn remove_meta_service_group_status(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: &str,
    ) -> Result<(), EndpointError> {
        Ok(txn.remove(paths.meta_service_group_status_path(id).as_bytes())?)
    }
}
