// Copyright 2026 AsterSQL.
use super::EtcdMetaStore;
use crate::Result;
impl EtcdMetaStore {
    /// Return the current backend consensus leader's advertised client URL.
    /// This is topology, not a PD identity or permission to administer a member.
    /// Reuses the host-owned SDK connection and performs no metadata mutations.
    pub async fn consensus_leader_client_url(&self) -> Result<Option<String>> {
        let mut client = self.client.clone();
        let members = self.bounded(client.member_list()).await?;
        let status = self.bounded(client.status()).await?;
        Ok(members
            .members()
            .iter()
            .find(|member| member.id() == status.leader())
            .and_then(|member| member.client_urls().first())
            .cloned())
    }
}
