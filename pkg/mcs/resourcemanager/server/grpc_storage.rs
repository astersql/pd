// Copyright 2026 AsterSQL.
use crate::grpc_codec::{decode, encode};
use crate::{ControllerConfig, Group, GroupState, Persistence};
use astersql_pd_keypath::{
    keyspace_resource_group_setting_path, keyspace_resource_group_state_path,
};
use astersql_pd_proto::resource_manager::ResourceGroup;
use astersql_pd_storage_domain::{MetaStore, Mutation};
use astersql_pd_storage_endpoint::StorageEndpoint;
use prost::Message;
use std::{collections::BTreeMap, sync::Arc};
pub(crate) struct Storage(pub Arc<dyn MetaStore>, pub ControllerConfig);
impl Storage {
    pub fn new(store: Arc<dyn MetaStore>) -> Self {
        let mut config = ControllerConfig::default();
        config.adjust(&Default::default());
        Self(store, config)
    }
}
#[async_trait::async_trait]
impl Persistence for Storage {
    async fn load(
        &self,
        role: crate::WriteRole,
    ) -> Result<Vec<(u32, Group, Option<GroupState>)>, String> {
        let endpoint = StorageEndpoint::new(self.0.clone());
        let bytes = endpoint
            .load_controller_config()
            .await
            .map_err(|e| e.to_string())?;
        let config = crate::rm_controller::restore(&self.1, &bytes)?;
        if role.metadata() {
            endpoint
                .save_controller_config(&config)
                .await
                .map_err(|e| e.to_string())?;
        }
        let mut groups = BTreeMap::new();
        endpoint
            .load_resource_group_settings(|id, name, bytes| {
                if let Ok(proto) = ResourceGroup::decode(bytes) {
                    if proto.name.as_bytes() == name {
                        groups.insert((id, proto.name.clone()), (decode(proto), None));
                    }
                }
            })
            .await
            .map_err(|e| e.to_string())?;
        endpoint
            .load_resource_group_states(|id, name, bytes| {
                if let Ok(name) = std::str::from_utf8(name) {
                    if let Some((_, state)) = groups.get_mut(&(id, name.to_string())) {
                        *state = serde_json::from_slice(bytes).ok();
                    }
                }
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(groups
            .into_iter()
            .map(|((id, _), (g, s))| (id, g, s))
            .collect())
    }
    async fn save_setting(&self, id: u32, g: &Group) -> Result<(), String> {
        StorageEndpoint::new(self.0.clone())
            .save_resource_group_setting(id, &g.name, &encode(g, id).encode_to_vec())
            .await
            .map_err(|e| e.to_string())
    }
    async fn save_state(&self, id: u32, name: &str, s: &GroupState) -> Result<(), String> {
        let mut floats = Vec::new();
        if let Some(bucket) = &s.ru {
            floats.push(bucket.tokens);
        }
        if let Some(stats) = &s.consumption {
            floats.extend([
                stats.r_r_u,
                stats.w_r_u,
                stats.read_bytes,
                stats.write_bytes,
                stats.total_cpu_time_ms,
                stats.sql_layer_cpu_time_ms,
                stats.kv_read_rpc_count,
                stats.kv_write_rpc_count,
                stats.tikv_r_u_v2,
                stats.tidb_r_u_v2,
                stats.tiflash_r_u_v2,
            ]);
        }
        if let Some(value) = floats.into_iter().find(|v| !v.is_finite()) {
            let value = if value.is_nan() {
                "NaN"
            } else if value > 0.0 {
                "+Inf"
            } else {
                "-Inf"
            };
            return Err(astersql_pd_errs::catalog::ERR_JSON_MARSHAL
                .with_source(
                    [] as [&str; 0],
                    std::io::Error::other(format!("json: unsupported value: {value}")),
                )
                .to_string());
        }
        StorageEndpoint::new(self.0.clone())
            .save_resource_group_states(id, name, s)
            .await
            .map_err(|e| e.to_string())
    }
    async fn delete(&self, id: u32, name: &str) -> Result<(), String> {
        let writes = [
            keyspace_resource_group_setting_path(id, name),
            keyspace_resource_group_state_path(id, name),
        ]
        .into_iter()
        .map(|key| Mutation::Delete {
            key: key.into_bytes(),
        })
        .collect();
        let result = self
            .0
            .transact(vec![], writes)
            .await
            .map_err(|e| e.to_string())?;
        if result.succeeded {
            Ok(())
        } else {
            Err("metadata backend transaction failed".into())
        }
    }
}
