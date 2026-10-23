// Copyright 2026 AsterSQL.
use std::ops::AsyncFnOnce;

use crate::{Base, Compare, Consistency, Entry, MetaStore, Mutation, Read, Result, StoreError};

/// Distinguishes the original callback error from commit failures and read conflicts.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CallbackTxnError<E> {
    #[error("transaction callback failed: {0}")]
    Callback(E),
    #[error(transparent)]
    Store(StoreError),
    #[error("transaction conflict")]
    Conflict,
}

/// Buffered writes and value-protected live reads, matching Go's etcdTxn.
/// Reads never see buffered writes and do not form a repeatable-read snapshot.
pub struct CallbackTxn<'a> {
    store: &'a dyn MetaStore,
    conditions: Vec<Compare>,
    operations: Vec<Mutation>,
}

impl<'a> Base<'a> {
    /// Run once and commit only after the callback succeeds. No automatic retry.
    pub async fn run_in_txn<T, E, F>(
        &self,
        callback: F,
    ) -> std::result::Result<T, CallbackTxnError<E>>
    where
        F: AsyncFnOnce(&mut CallbackTxn<'a>) -> std::result::Result<T, E>,
    {
        self.run_in_txn_with_conditions(vec![], callback).await
    }

    /// Additional domain comparisons are evaluated atomically with read guards and writes.
    pub async fn run_in_txn_with_conditions<T, E, F>(
        &self,
        conditions: Vec<Compare>,
        callback: F,
    ) -> std::result::Result<T, CallbackTxnError<E>>
    where
        F: AsyncFnOnce(&mut CallbackTxn<'a>) -> std::result::Result<T, E>,
    {
        let mut txn = CallbackTxn {
            store: self.store,
            conditions,
            operations: vec![],
        };
        let value = callback(&mut txn)
            .await
            .map_err(CallbackTxnError::Callback)?;
        let commit = self
            .store
            .transact(txn.conditions, txn.operations)
            .await
            .map_err(CallbackTxnError::Store)?;
        if !commit.succeeded {
            return Err(CallbackTxnError::Conflict);
        }
        Ok(value)
    }
}

impl CallbackTxn<'_> {
    /// Buffer a put without writing to the backend.
    pub fn save(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        self.operations.push(Mutation::put(key, value));
        Ok(())
    }

    /// Buffer a single-key delete.
    pub fn remove(&mut self, key: &[u8]) -> Result<()> {
        self.operations.push(Mutation::Delete { key: key.to_vec() });
        Ok(())
    }

    /// Guard an existing value by bytes; guard a missing key by create revision zero.
    pub async fn load(&mut self, key: &[u8]) -> Result<Vec<u8>> {
        self.load_value(key, false).await
    }

    pub(crate) async fn load_value(&mut self, key: &[u8], revision: bool) -> Result<Vec<u8>> {
        let snapshot = self
            .store
            .read(Read::range(key, b"", Consistency::Linearizable))
            .await?;
        match snapshot.entries.as_slice() {
            [] => {
                self.conditions
                    .push(Compare::CreateRevision(key.to_vec(), 0));
                Ok(vec![])
            }
            [entry] => {
                if revision {
                    self.conditions
                        .push(Compare::ModRevision(entry.key.clone(), entry.mod_revision));
                } else {
                    self.protect(entry);
                }
                Ok(entry.value.clone())
            }
            _ => Err(StoreError::Backend),
        }
    }

    /// Protect only returned entries; no protection for phantoms or keys beyond the limit.
    pub async fn load_range(&mut self, key: &[u8], end: &[u8], limit: usize) -> Result<Vec<Entry>> {
        let mut read = Read::range(key, end, Consistency::Linearizable);
        read.limit = limit;
        let snapshot = self.store.read(read).await?;
        for entry in &snapshot.entries {
            self.protect(entry);
        }
        Ok(snapshot.entries)
    }

    fn protect(&mut self, entry: &Entry) {
        self.conditions
            .push(Compare::Value(entry.key.clone(), entry.value.clone()));
    }
}
