// Copyright 2026 AsterSQL.
use crate::{CallbackTxn, MetaStore, StoreError};
use astersql_pd_keypath::ClusterPaths;
use std::{collections::HashMap, sync::Arc};
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
}
/// Keyspace codec, name-index parsing and storage errors remain distinguishable.
#[derive(Debug, thiserror::Error)]
pub enum KeyspaceMetaError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("proto unmarshal: {0}")]
    ProtoUnmarshal(Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Decode(Box<dyn std::error::Error + Send + Sync>),
    #[error("invalid decimal uint24 keyspace id")]
    InvalidId,
}

/// Host-provided persistence codec; domain APIs never depend on generated messages.
pub trait KeyspaceCodec: Send + Sync {
    fn encode(&self, meta: &Keyspace) -> Vec<u8>;
    fn decode(&self, bytes: &[u8]) -> Result<Keyspace, Box<dyn std::error::Error + Send + Sync>>;
}
#[derive(Clone)]
pub struct KeyspaceStorage {
    pub store: Arc<dyn MetaStore>,
    pub paths: ClusterPaths,
    codec: Arc<dyn KeyspaceCodec>,
}
impl KeyspaceStorage {
    pub fn new(
        store: Arc<dyn MetaStore>,
        paths: ClusterPaths,
        codec: Arc<dyn KeyspaceCodec>,
    ) -> Self {
        Self {
            store,
            paths,
            codec,
        }
    }
}
impl KeyspaceStorage {
    /// Buffer metadata in the caller's transaction; does not create the name index.
    pub fn save_keyspace_meta(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        meta: &Keyspace,
    ) -> Result<(), KeyspaceMetaError> {
        let bytes = self.codec.encode(meta);
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
        Ok(Some(
            self.codec
                .decode(&bytes)
                .map_err(KeyspaceMetaError::ProtoUnmarshal)?,
        ))
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
                self.codec
                    .decode(&entry.value)
                    .map_err(KeyspaceMetaError::Decode)
            })
            .collect()
    }
}
