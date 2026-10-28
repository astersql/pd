// Copyright 2026 AsterSQL.
//! Host-owned region state operations; wire types remain in the server adapter.
use astersql_pd_core::{NodeState, RegionInfo};

/// State acceptance failure sent in a heartbeat error header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionHeartbeatError(pub String);

/// Host supplies membership, bootstrap state, flow configuration and canonical
/// region admission/side effects. The command routing remains host-owned.
#[async_trait::async_trait]
pub trait RegionHeartbeatService: Send + Sync {
    fn is_bootstrapped(&self) -> bool {
        true
    }
    fn region_store_state(&self, store_id: u64) -> Option<NodeState>;
    fn flow_round_divisor(&self) -> u64 {
        1
    }
    /// Bind the transport for subsequent operator commands to this store.
    async fn bind_heartbeat(
        &self,
        _store_id: u64,
        _stream: std::sync::Arc<dyn astersql_pd_schedule_hbstream::routing::HeartbeatStream>,
    ) -> Result<(), RegionHeartbeatError> {
        Ok(())
    }
    async fn region_heartbeat(&self, region: RegionInfo) -> Result<(), RegionHeartbeatError>;
}
