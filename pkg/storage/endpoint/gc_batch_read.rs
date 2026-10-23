// Copyright 2026 AsterSQL.

use crate::gc_safepoint_read::{decode_gc_safe_point, decode_txn_safe_point_parts};
use crate::{SafePointReadError, StorageEndpoint};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{RawTxn, RawTxnOp, RawTxnResponse, StoreError};
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

/// Go reserves two operations for each of at most 60 scopes.
pub const MAX_GC_SAFE_POINT_BATCH_SIZE: usize = 60;

/// Values are valid only when `err` is absent, matching Go's per-scope result.
#[derive(Debug)]
pub struct GCSafePointReadResult {
    pub keyspace_id: u32,
    pub txn_safe_point: u64,
    pub gc_safe_point: u64,
    pub err: Option<SafePointReadError>,
}

/// Input and whole-transaction failures; decode errors belong to individual rows.
#[derive(Debug, thiserror::Error)]
pub enum GCSafePointBatchError {
    #[error("gc safe point batch exceeds maximum size 60")]
    TooLarge,
    #[error("duplicate keyspace ID {0} in GC safe point batch")]
    Duplicate(u32),
    #[error("invalid GC safe point batch response")]
    InvalidResponse,
    #[error("invalid GC safe point read response for key {0:?}")]
    InvalidRead(Vec<u8>),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl StorageEndpoint {
    /// Reads both safe points atomically, preserving input order and row errors.
    pub async fn load_gc_safe_point_pairs(
        &self,
        paths: &ClusterPaths,
        ids: &[u32],
        cancel: &CancellationToken,
    ) -> Result<Vec<GCSafePointReadResult>, GCSafePointBatchError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        if ids.len() > MAX_GC_SAFE_POINT_BATCH_SIZE {
            return Err(GCSafePointBatchError::TooLarge);
        }
        let mut seen = HashSet::with_capacity(ids.len());
        let mut keys = Vec::with_capacity(ids.len() * 2);
        for &id in ids {
            if !seen.insert(id) {
                return Err(GCSafePointBatchError::Duplicate(id));
            }
            keys.push(paths.txn_safe_point_path(id).into_bytes());
            keys.push(paths.gc_safe_point_path(id).into_bytes());
        }
        if cancel.is_cancelled() {
            return Err(StoreError::Cancelled.into());
        }
        let txn = RawTxn::new(self.store.as_ref())
            .then_ops(keys.iter().cloned().map(RawTxnOp::Get).collect());
        // A read-only transaction has no unknown mutation outcome on cancellation.
        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StoreError::Cancelled.into()),
            response = txn.commit() => response?,
        };
        decode_response(ids, &keys, response)
    }
}

// Validate the entire transport response before decoding any row.
pub(super) fn decode_response(
    ids: &[u32],
    keys: &[Vec<u8>],
    response: RawTxnResponse,
) -> Result<Vec<GCSafePointReadResult>, GCSafePointBatchError> {
    if !response.succeeded || response.responses.len() != keys.len() {
        return Err(GCSafePointBatchError::InvalidResponse);
    }
    for (item, key) in response.responses.iter().zip(keys) {
        if !item.key_value_pairs.is_empty()
            && (item.key_value_pairs.len() != 1 || item.key_value_pairs[0].key != *key)
        {
            return Err(GCSafePointBatchError::InvalidRead(key.clone()));
        }
    }
    let value = |i: usize| {
        response.responses[i]
            .key_value_pairs
            .first()
            .map_or(&[][..], |pair| pair.value.as_slice())
    };
    Ok(ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let mut row = GCSafePointReadResult {
                keyspace_id: id,
                txn_safe_point: 0,
                gc_safe_point: 0,
                err: None,
            };
            (row.txn_safe_point, row.err) = decode_txn_safe_point_parts(value(2 * i));
            if row.err.is_none() {
                match decode_gc_safe_point(value(2 * i + 1), id) {
                    Ok(gc) => row.gc_safe_point = gc,
                    Err(err) => row.err = Some(err),
                }
            }
            row
        })
        .collect())
}
