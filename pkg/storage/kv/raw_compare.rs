// Copyright 2026 AsterSQL.
use crate::Compare;

/// Go RawTxn comparison kinds. Values are compared as unsigned byte sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawTxnCmpType {
    Equal,
    NotEqual,
    Less,
    Greater,
    Exists,
    NotExists,
}

/// A condition evaluated by the backend at the atomic transaction commit point.
/// Exists and NotExists ignore value, distinguishing a missing key from an empty value.
#[derive(Debug, Clone)]
pub struct RawTxnCondition {
    pub key: Vec<u8>,
    pub cmp_type: RawTxnCmpType,
    pub value: Vec<u8>,
}

impl From<RawTxnCondition> for Compare {
    fn from(condition: RawTxnCondition) -> Self {
        let RawTxnCondition {
            key,
            cmp_type,
            value,
        } = condition;
        match cmp_type {
            RawTxnCmpType::Equal => Self::Value(key, value),
            RawTxnCmpType::NotEqual => Self::ValueNotEqual(key, value),
            RawTxnCmpType::Less => Self::ValueLess(key, value),
            RawTxnCmpType::Greater => Self::ValueGreater(key, value),
            RawTxnCmpType::Exists => Self::Exists(key),
            RawTxnCmpType::NotExists => Self::NotExists(key),
        }
    }
}
