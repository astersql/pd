// Copyright 2026 AsterSQL.
use crate::PdServices;
use astersql_pd_rm_server::{Host, Manager};
use std::sync::Arc;
pub(crate) struct ResourceManagerHost(pub Arc<dyn PdServices>);
#[async_trait::async_trait]
impl Host for ResourceManagerHost {
    async fn consumption_keyspace_exists(&self, id: u32) -> bool {
        let Ok(manager) = self.0.keyspace_manager().await else {
            return false;
        };
        manager
            .load_by_id(id)
            .await
            .is_ok_and(|keyspace| !keyspace.name.is_empty())
    }

    async fn manager(&self) -> Result<Arc<tokio::sync::Mutex<Manager>>, tonic::Status> {
        self.0
            .resource_groups()
            .await
            .map_err(|_| tonic::Status::unavailable("not leader"))
    }
}
