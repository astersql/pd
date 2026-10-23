// Copyright 2026 AsterSQL.
use crate::{Consistency, MetaStore, Mutation, Read, Result, StoreError};

/// Backend-neutral Go Base single-key operations over the canonical MetaStore.
pub struct Base<'a> {
    pub(crate) store: &'a dyn MetaStore,
}

impl<'a> Base<'a> {
    pub fn new(store: &'a dyn MetaStore) -> Self {
        Self { store }
    }

    /// Load a value; missing keys and stored empty values both return empty bytes.
    pub async fn load(&self, key: &[u8]) -> Result<Vec<u8>> {
        let snapshot = self
            .store
            .read(Read::range(key, b"", Consistency::Linearizable))
            .await?;
        match snapshot.entries.as_slice() {
            [] => Ok(Vec::new()),
            [entry] => Ok(entry.value.clone()),
            _ => Err(StoreError::Backend),
        }
    }

    /// Save overwrites exactly one key without attaching a lease.
    pub async fn save(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.write(Mutation::put(key, value)).await
    }

    /// Remove exactly one key; removing a missing key succeeds.
    pub async fn remove(&self, key: &[u8]) -> Result<()> {
        self.write(Mutation::Delete { key: key.to_vec() }).await
    }

    async fn write(&self, mutation: Mutation) -> Result<()> {
        let commit = self.store.transact(vec![], vec![mutation]).await?;
        if !commit.succeeded {
            return Err(StoreError::Backend);
        }
        Ok(())
    }
}
