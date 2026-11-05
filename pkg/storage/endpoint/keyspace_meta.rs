// Copyright 2026 AsterSQL.
use crate::StorageEndpoint;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{apipb, keyspacepb};
use astersql_pd_storage_domain::{CallbackTxn, Keyspace, KeyspaceIdentity, KeyspaceMetaError};
use prost::Message;
fn wire(meta: &Keyspace) -> keyspacepb::KeyspaceMeta {
    use keyspacepb::keyspace_meta::Keyspace as Identity;
    keyspacepb::KeyspaceMeta {
        keyspace: meta.identity.as_ref().map(|identity| match *identity {
            KeyspaceIdentity::Id(id) => Identity::Id(id),
            KeyspaceIdentity::Scoped {
                namespace_id,
                keyspace_id,
            } => Identity::KeyspaceIdentity(apipb::KeyspaceIdentity {
                namespace_id,
                keyspace_id,
            }),
        }),
        name: meta.name.clone(),
        state: meta.state,
        created_at: meta.created_at,
        state_changed_at: meta.state_changed_at,
        config: meta.config.clone(),
    }
}
fn from_wire(wire: keyspacepb::KeyspaceMeta) -> Keyspace {
    use keyspacepb::keyspace_meta::Keyspace as Identity;
    Keyspace {
        identity: wire.keyspace.map(|identity| match identity {
            Identity::Id(id) => KeyspaceIdentity::Id(id),
            Identity::KeyspaceIdentity(id) => KeyspaceIdentity::Scoped {
                namespace_id: id.namespace_id,
                keyspace_id: id.keyspace_id,
            },
        }),
        name: wire.name,
        state: wire.state,
        created_at: wire.created_at,
        state_changed_at: wire.state_changed_at,
        config: wire.config,
    }
}
#[derive(Default)]
pub struct ProtobufKeyspaceCodec;
impl astersql_pd_storage_domain::KeyspaceCodec for ProtobufKeyspaceCodec {
    fn encode(&self, meta: &Keyspace) -> Vec<u8> {
        let mut wire = wire(meta);
        // gogo emits oneof after ordinary fields.
        let identity = wire.keyspace.take();
        let mut bytes = wire.encode_to_vec();
        bytes.extend(
            keyspacepb::KeyspaceMeta {
                keyspace: identity,
                ..Default::default()
            }
            .encode_to_vec(),
        );
        bytes
    }
    fn decode(&self, bytes: &[u8]) -> Result<Keyspace, Box<dyn std::error::Error + Send + Sync>> {
        Ok(from_wire(keyspacepb::KeyspaceMeta::decode(bytes)?))
    }
}
impl StorageEndpoint {
    pub fn save_keyspace_meta(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        meta: &Keyspace,
    ) -> Result<(), KeyspaceMetaError> {
        self.keyspace_storage(paths.clone())
            .save_keyspace_meta(txn, paths, meta)
    }
    pub fn keyspace_storage(
        &self,
        paths: ClusterPaths,
    ) -> astersql_pd_storage_domain::KeyspaceStorage {
        astersql_pd_storage_domain::KeyspaceStorage::new(
            self.store.clone(),
            paths,
            std::sync::Arc::new(ProtobufKeyspaceCodec),
        )
    }
    pub async fn load_keyspace_meta(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: u32,
    ) -> Result<Option<Keyspace>, KeyspaceMetaError> {
        self.keyspace_storage(paths.clone())
            .load_keyspace_meta(txn, paths, id)
            .await
    }
    pub fn save_keyspace_id(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: u32,
        name: &str,
    ) -> Result<(), KeyspaceMetaError> {
        self.keyspace_storage(paths.clone())
            .save_keyspace_id(txn, paths, id, name)
    }
    pub async fn load_keyspace_id(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        name: &str,
    ) -> Result<Option<u32>, KeyspaceMetaError> {
        self.keyspace_storage(paths.clone())
            .load_keyspace_id(txn, paths, name)
            .await
    }
    pub fn remove_keyspace(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: u32,
        name: &str,
    ) -> Result<(), KeyspaceMetaError> {
        self.keyspace_storage(paths.clone())
            .remove_keyspace(txn, paths, id, name)
    }
    pub async fn load_range_keyspace(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        start_id: u32,
        limit: usize,
    ) -> Result<Vec<Keyspace>, KeyspaceMetaError> {
        self.keyspace_storage(paths.clone())
            .load_range_keyspace(txn, paths, start_id, limit)
            .await
    }
}
