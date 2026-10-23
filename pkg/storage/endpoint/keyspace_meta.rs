// Copyright 2026 AsterSQL.

use crate::StorageEndpoint;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{apipb, keyspacepb};
use astersql_pd_storage_kv::{CallbackTxn, StoreError};
use prost::Message;
use std::collections::HashMap;

/// Keyspace identity without protocol types; absence differs from legacy ID zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyspaceIdentity {
    /// Legacy ID used by Go's GetId and storage key selection.
    Id(u32),
    /// V3 identity; Go GetId returns zero for this variant.
    Scoped { namespace_id: u32, keyspace_id: u32 },
}

/// Persisted keyspace metadata, independent of transport messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keyspace {
    pub identity: Option<KeyspaceIdentity>,
    pub name: String,
    /// Raw enum number preserves unknown protocol values.
    pub state: i32,
    pub created_at: i64,
    pub state_changed_at: i64,
    pub config: HashMap<String, String>,
}
impl Keyspace {
    /// Match Go KeyspaceMeta.GetId, including absent and V3 identities.
    pub fn id(&self) -> u32 {
        match self.identity {
            Some(KeyspaceIdentity::Id(id)) => id,
            _ => 0,
        }
    }
    fn wire(&self) -> keyspacepb::KeyspaceMeta {
        use keyspacepb::keyspace_meta::Keyspace as Identity;
        keyspacepb::KeyspaceMeta {
            keyspace: self.identity.as_ref().map(|identity| match *identity {
                KeyspaceIdentity::Id(id) => Identity::Id(id),
                KeyspaceIdentity::Scoped {
                    namespace_id,
                    keyspace_id,
                } => Identity::KeyspaceIdentity(apipb::KeyspaceIdentity {
                    namespace_id,
                    keyspace_id,
                }),
            }),
            name: self.name.clone(),
            state: self.state,
            created_at: self.created_at,
            state_changed_at: self.state_changed_at,
            config: self.config.clone(),
        }
    }
    fn from_wire(wire: keyspacepb::KeyspaceMeta) -> Self {
        use keyspacepb::keyspace_meta::Keyspace as Identity;
        Self {
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
}

/// Keyspace codec, name-index parsing and storage errors remain distinguishable.
#[derive(Debug, thiserror::Error)]
pub enum KeyspaceMetaError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("proto unmarshal: {0}")]
    ProtoUnmarshal(prost::DecodeError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error("invalid decimal uint24 keyspace id")]
    InvalidId,
}

impl StorageEndpoint {
    /// Buffer metadata in the caller's transaction; does not create the name index.
    pub fn save_keyspace_meta(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        meta: &Keyspace,
    ) -> Result<(), KeyspaceMetaError> {
        let mut wire = meta.wire();
        // gogo emits the oneof after ordinary fields. Both portions use the pinned codec.
        let identity = wire.keyspace.take();
        let mut bytes = wire.encode_to_vec();
        bytes.extend(
            keyspacepb::KeyspaceMeta {
                keyspace: identity,
                ..Default::default()
            }
            .encode_to_vec(),
        );
        txn.save(paths.keyspace_meta_path(meta.id()).as_bytes(), &bytes)?;
        Ok(())
    }
    /// Read metadata with revision protection, matching Go RevisionTxn preference.
    pub async fn load_keyspace_meta(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: u32,
    ) -> Result<Option<Keyspace>, KeyspaceMetaError> {
        let bytes = txn
            .load_with_revision(paths.keyspace_meta_path(id).as_bytes())
            .await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(Keyspace::from_wire(
            keyspacepb::KeyspaceMeta::decode(bytes.as_slice())
                .map_err(KeyspaceMetaError::ProtoUnmarshal)?,
        )))
    }
    /// Buffer a decimal name index; saving itself does not restrict the ID to uint24.
    pub fn save_keyspace_id(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: u32,
        name: &str,
    ) -> Result<(), KeyspaceMetaError> {
        txn.save(
            paths.keyspace_id_path(name).as_bytes(),
            id.to_string().as_bytes(),
        )?;
        Ok(())
    }
    /// Missing and empty are absent; a present decimal zero is Some(0).
    pub async fn load_keyspace_id(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        name: &str,
    ) -> Result<Option<u32>, KeyspaceMetaError> {
        let bytes = txn.load(paths.keyspace_id_path(name).as_bytes()).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        if !bytes.iter().all(u8::is_ascii_digit) {
            return Err(KeyspaceMetaError::InvalidId);
        }
        let id = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|id| *id < 1 << 24)
            .ok_or(KeyspaceMetaError::InvalidId)?;
        Ok(Some(id))
    }
    /// Buffer removal of metadata and the supplied name in one caller transaction.
    pub fn remove_keyspace(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: u32,
        name: &str,
    ) -> Result<(), KeyspaceMetaError> {
        txn.remove(paths.keyspace_meta_path(id).as_bytes())?;
        txn.remove(paths.keyspace_id_path(name).as_bytes())?;
        Ok(())
    }
    /// Ordered metadata range, inclusive start; zero limit means unbounded.
    pub async fn load_range_keyspace(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        start_id: u32,
        limit: usize,
    ) -> Result<Vec<Keyspace>, KeyspaceMetaError> {
        let mut end = paths.keyspace_meta_prefix().into_bytes();
        *end.last_mut().expect("nonempty prefix") += 1;
        let entries = txn
            .load_range(paths.keyspace_meta_path(start_id).as_bytes(), &end, limit)
            .await?;
        entries
            .into_iter()
            .map(|entry| {
                keyspacepb::KeyspaceMeta::decode(entry.value.as_slice())
                    .map(Keyspace::from_wire)
                    .map_err(KeyspaceMetaError::from)
            })
            .collect()
    }
}
