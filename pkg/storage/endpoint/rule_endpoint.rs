// Copyright 2026 AsterSQL.
use crate::kv::{Base, CallbackTxn, CallbackTxnError, Consistency, Read};
use crate::{EndpointError, StorageEndpoint};
use astersql_pd_keypath::ClusterPaths;
use serde::Serialize;
use std::ops::AsyncFnOnce;

impl StorageEndpoint {
    /// Run one buffered transaction; callback errors discard all writes, without retry.
    pub async fn run_in_txn<T, E, F>(&self, callback: F) -> Result<T, CallbackTxnError<E>>
    where
        F: AsyncFnOnce(&mut CallbackTxn<'_>) -> Result<T, E>,
    {
        Base::new(self.store.as_ref()).run_in_txn(callback).await
    }

    /// Load raw placement-rule bytes; missing and empty values collapse to empty.
    pub async fn load_rule(
        &self,
        paths: &ClusterPaths,
        name: &str,
    ) -> Result<Vec<u8>, EndpointError> {
        Ok(Base::new(self.store.as_ref())
            .load(paths.rule_key_path(name).as_bytes())
            .await?)
    }
    /// Encode a caller-owned persistence model before buffering its put.
    /// Models must supply Go-compatible field names and JSON encoding.
    pub fn save_rule<T: Serialize + ?Sized>(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        name: &str,
        value: &T,
    ) -> Result<(), EndpointError> {
        let bytes = serde_json::to_vec(value).map_err(EndpointError::JsonMarshal)?;
        Ok(txn.save(paths.rule_key_path(name).as_bytes(), &bytes)?)
    }
    /// Buffer deletion of exactly one key; a missing key is harmless.
    pub fn delete_rule(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        name: &str,
    ) -> Result<(), EndpointError> {
        Ok(txn.remove(paths.rule_key_path(name).as_bytes())?)
    }
    /// Encode a caller-owned persistence model before buffering its put.
    /// Models must supply Go-compatible field names and JSON encoding.
    pub fn save_rule_group<T: Serialize + ?Sized>(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        name: &str,
        value: &T,
    ) -> Result<(), EndpointError> {
        let bytes = serde_json::to_vec(value).map_err(EndpointError::JsonMarshal)?;
        Ok(txn.save(paths.rule_group_id_path(name).as_bytes(), &bytes)?)
    }
    /// Buffer deletion of exactly one key; a missing key is harmless.
    pub fn delete_rule_group(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        name: &str,
    ) -> Result<(), EndpointError> {
        Ok(txn.remove(paths.rule_group_id_path(name).as_bytes())?)
    }
    /// Encode a caller-owned persistence model before buffering its put.
    /// Models must supply Go-compatible field names and JSON encoding.
    pub fn save_region_rule<T: Serialize + ?Sized>(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        name: &str,
        value: &T,
    ) -> Result<(), EndpointError> {
        let bytes = serde_json::to_vec(value).map_err(EndpointError::JsonMarshal)?;
        Ok(txn.save(paths.region_label_key_path(name).as_bytes(), &bytes)?)
    }
    /// Buffer deletion of exactly one key; a missing key is harmless.
    pub fn delete_region_rule(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        name: &str,
    ) -> Result<(), EndpointError> {
        Ok(txn.remove(paths.region_label_key_path(name).as_bytes())?)
    }
    /// Deliver raw suffix/value pairs in byte order, reading pages of 100.
    /// Callbacks from earlier pages remain delivered if a later read fails.
    pub async fn load_rules(
        &self,
        paths: &ClusterPaths,
        callback: impl FnMut(&[u8], &[u8]),
    ) -> Result<(), EndpointError> {
        self.load_rule_prefix(paths.rules_path_prefix().as_bytes(), callback)
            .await
    }
    /// Deliver raw suffix/value pairs in byte order, reading pages of 100.
    /// Callbacks from earlier pages remain delivered if a later read fails.
    pub async fn load_rule_groups(
        &self,
        paths: &ClusterPaths,
        callback: impl FnMut(&[u8], &[u8]),
    ) -> Result<(), EndpointError> {
        self.load_rule_prefix(paths.rule_group_path_prefix().as_bytes(), callback)
            .await
    }
    /// Deliver raw suffix/value pairs in byte order, reading pages of 100.
    /// Callbacks from earlier pages remain delivered if a later read fails.
    pub async fn load_region_rules(
        &self,
        paths: &ClusterPaths,
        callback: impl FnMut(&[u8], &[u8]),
    ) -> Result<(), EndpointError> {
        self.load_rule_prefix(paths.region_label_path_prefix().as_bytes(), callback)
            .await
    }
    /// Adapter seam for ordered raw prefix scans with the same paging and error semantics.
    pub async fn load_rule_prefix(
        &self,
        prefix: &[u8],
        mut callback: impl FnMut(&[u8], &[u8]),
    ) -> Result<(), EndpointError> {
        let mut request = Read::prefix(prefix, Consistency::Linearizable);
        request.limit = 100;
        loop {
            let snapshot = self.store.read(request.clone()).await?;
            for entry in &snapshot.entries {
                callback(&entry.key[prefix.len()..], &entry.value);
            }
            if snapshot.entries.len() < 100 {
                return Ok(());
            }
            let mut next = snapshot.entries.last().expect("full page").key.clone();
            next.push(0);
            request.start = Some(next);
        }
    }
}
