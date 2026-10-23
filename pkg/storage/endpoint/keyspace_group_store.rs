// Copyright 2026 AsterSQL.

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{CallbackTxn, Consistency, Read};

use crate::{EndpointError, KeyspaceGroup, StorageEndpoint};

/// Storage/codec failures and Go's default-group protection remain distinct.
#[derive(Debug, thiserror::Error)]
pub enum KeyspaceGroupStoreError {
    #[error(transparent)]
    Endpoint(#[from] EndpointError),
    #[error("[PD:keyspace:ErrModifyDefaultKeyspaceGroup]default keyspace group cannot be modified")]
    ModifyDefaultKeyspaceGroup,
}

impl StorageEndpoint {
    /// Read with a modification-revision guard; absent/empty bytes return None.
    pub async fn load_keyspace_group(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: u32,
    ) -> Result<Option<KeyspaceGroup>, KeyspaceGroupStoreError> {
        let bytes = txn
            .load_with_revision(paths.keyspace_group_id_path(id).as_bytes())
            .await
            .map_err(EndpointError::from)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(EndpointError::JsonUnmarshal)
            .map_err(Into::into)
    }

    /// Buffer Go-compatible JSON in the caller's transaction.
    pub fn save_keyspace_group(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        group: &KeyspaceGroup,
    ) -> Result<(), KeyspaceGroupStoreError> {
        // Go encoding/json escapes HTML and the two JavaScript line separators.
        let json = serde_json::to_string(group)
            .map_err(EndpointError::JsonMarshal)?
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        txn.save(
            paths.keyspace_group_id_path(group.id).as_bytes(),
            json.as_bytes(),
        )
        .map_err(EndpointError::from)?;
        Ok(())
    }

    /// Buffer removal; deleting a missing non-default group is successful.
    pub fn delete_keyspace_group(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        id: u32,
    ) -> Result<(), KeyspaceGroupStoreError> {
        if id == 0 {
            return Err(KeyspaceGroupStoreError::ModifyDefaultKeyspaceGroup);
        }
        txn.remove(paths.keyspace_group_id_path(id).as_bytes())
            .map_err(EndpointError::from)?;
        Ok(())
    }

    /// Inclusive start, lexicographic physical-key ordering; zero limit means all.
    /// Like Go this range read is outside the caller's transaction.
    pub async fn load_keyspace_groups(
        &self,
        paths: &ClusterPaths,
        start_id: u32,
        limit: usize,
    ) -> Result<Vec<KeyspaceGroup>, KeyspaceGroupStoreError> {
        let mut end = paths.keyspace_group_id_prefix().into_bytes();
        *end.last_mut().expect("nonempty membership prefix") += 1;
        let mut read = Read::range(
            paths.keyspace_group_id_path(start_id).as_bytes(),
            &end,
            Consistency::Linearizable,
        );
        read.limit = limit;
        self.store
            .read(read)
            .await
            .map_err(EndpointError::from)?
            .entries
            .into_iter()
            .map(|entry| {
                serde_json::from_slice(&entry.value)
                    .map_err(EndpointError::JsonUnmarshal)
                    .map_err(Into::into)
            })
            .collect()
    }
}
