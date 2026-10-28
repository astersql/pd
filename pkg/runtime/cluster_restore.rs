// Copyright 2026 AsterSQL.
//! Recover through the canonical persistence adapters before exposing the node.
use astersql_pd_core::BasicCluster;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::{ProviderSnapshot, ScheduleDefaults};
use astersql_pd_server_config::{ConfigDefaultsContext, PersistedConfig};
use astersql_pd_storage::MetaStore;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub(crate) async fn restore(
    mut cluster: BasicCluster,
    store: Arc<dyn MetaStore>,
    identity: &crate::NodeConfig,
    cancel: &CancellationToken,
) -> Result<(BasicCluster, ProviderSnapshot<()>, Option<PersistedConfig>), crate::NodeError> {
    let recover = async {
        let paths = ClusterPaths::new(identity.cluster_id);
        let mut endpoint = StorageEndpoint::new(store);
        let mut persisted = PersistedConfig::default();
        let cwd = std::env::current_dir()?;
        let found = persisted
            .reload(
                &endpoint,
                &paths,
                ConfigDefaultsContext {
                    hostname: &identity.name,
                    cwd: &cwd,
                },
                &mut ScheduleDefaults::default(),
            )
            .await?;
        let mut config = ProviderSnapshot::default();
        // Preserve the host's existing no-rule default until a persisted setting exists.
        config.replication.enable_placement_rules = false;
        if found {
            config.schedule = persisted.config.schedule.clone();
            config.replication = persisted.config.replication.clone();
            config.store = persisted.store.clone();
            config.flow_round_by_digit = persisted.config.pd_server.flow_round_by_digit;
            config.key_type = match persisted.config.pd_server.key_type.trim() {
                "" | "table" => astersql_pd_core_constant::KeyType::TABLE,
                "raw" => astersql_pd_core_constant::KeyType::RAW,
                "txn" => astersql_pd_core_constant::KeyType::TXN,
                _ => return Err("invalid persisted key type".into()),
            };
            config.label_properties = persisted
                .config
                .label_property
                .as_ref()
                .into_iter()
                .flat_map(|properties| properties.iter())
                .map(|(key, labels)| {
                    (
                        key.clone(),
                        labels
                            .as_deref()
                            .unwrap_or_default()
                            .iter()
                            .map(|label| astersql_pd_core::StoreLabel {
                                key: label.key.clone(),
                                value: label.value.clone(),
                            })
                            .collect(),
                    )
                })
                .collect();
        }
        endpoint
            .load_stores(&paths, |store| cluster.stores.put_store(Arc::new(store)))
            .await?;
        endpoint
            .load_regions(&paths, cancel, |region| {
                cluster
                    .regions
                    .check_and_put_region(Arc::new(region))
                    .into_iter()
                    .map(|r| r.as_ref().clone())
                    .collect()
            })
            .await?;
        cluster.update_all_store_status();
        Ok((cluster, config, found.then_some(persisted)))
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => Err("cluster recovery cancelled".into()),
        result = recover => result,
    }
}
