// Copyright 2026 AsterSQL.
use crate::*;

/// Backend-neutral operations in a RawTxn branch.
/// Reading and writing the same key in one transaction is unspecified by Go RawTxn.
#[derive(Debug, Clone)]
pub enum RawTxnOp {
    Write(Mutation),
    Get(Vec<u8>),
    /// Ordered range with Go RawTxn end/limit semantics.
    GetRange {
        key: Vec<u8>,
        end: Vec<u8>,
        limit: usize,
    },
}
impl From<Mutation> for RawTxnOp {
    fn from(value: Mutation) -> Self {
        Self::Write(value)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValuePair {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}
/// One item per selected operation; writes and missing gets have no pairs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawTxnResponseItem {
    pub key_value_pairs: Vec<KeyValuePair>,
}
#[derive(Debug, Clone)]
pub struct RawTxnResponse {
    pub succeeded: bool,
    pub revision: Revision,
    pub responses: Vec<RawTxnResponseItem>,
}

/// Validate both branches before execution, including unselected reads.
pub(crate) fn validate_raw(compares: &[Compare], ops: &[RawTxnOp]) -> Result<()> {
    if compares.len() + ops.len() > 128
        || ops
            .iter()
            .any(|op| matches!(op, RawTxnOp::Get(key) if key.is_empty()))
    {
        return Err(StoreError::InvalidArgument);
    }
    for op in ops {
        if let RawTxnOp::GetRange { key, end, limit } = op {
            crate::raw_range::request(key, end, *limit).validate()?;
        }
    }
    let writes: Vec<_> = ops
        .iter()
        .filter_map(|op| match op {
            RawTxnOp::Write(w) => Some(w.clone()),
            RawTxnOp::Get(_) | RawTxnOp::GetRange { .. } => None,
        })
        .collect();
    crate::meta_store::validate_txn(compares, &writes)
}
