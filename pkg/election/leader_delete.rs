// Copyright 2026 AsterSQL.
use crate::{ElectionError, Leadership, Result};
use astersql_pd_storage_kv::{Compare, Mutation, RawTxnOp, Revision, StoreError};

impl Leadership {
    /// Delete exactly the observed leader revision, or succeed if it is already absent.
    /// A conflicting live key leaves this term's lease and local expiry unchanged.
    /// Like resign, cleanup remains available after the caller cancels the term.
    /// Success stops renewal and revokes only this immutable term's lease.
    pub async fn delete_leader_key_by_revision(&self, revision: Revision) -> Result<()> {
        let response = self
            .store
            .transact_raw(
                vec![Compare::ModRevision(self.key.clone(), revision)],
                vec![RawTxnOp::Write(Mutation::Delete {
                    key: self.key.clone(),
                })],
                vec![RawTxnOp::Get(self.key.clone())],
            )
            .await?;
        if !response.succeeded {
            let [entry] = response.responses.as_slice() else {
                return Err(StoreError::Backend.into());
            };
            if !entry.key_value_pairs.is_empty() {
                return Err(ElectionError::Contended);
            }
        }
        self.resign().await
    }
}
