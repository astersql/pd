// Copyright 2026 AsterSQL.
use crate::PdServices;
use astersql_pd_mcs_metastorage_server::Host;
use astersql_pd_storage_domain::MetaStore;
use std::sync::Arc;
pub(crate) struct MetastorageHost(pub Arc<dyn PdServices>);
#[async_trait::async_trait]
impl Host for MetastorageHost {
    fn cluster_id(&self) -> u64 {
        self.0.identity().cluster_id
    }
    fn store(&self) -> Option<Arc<dyn MetaStore>> {
        self.0.discovery_store()
    }
    async fn is_serving(&self) -> bool {
        self.0.healthy().await.is_ok()
    }
}
