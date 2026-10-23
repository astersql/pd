// Copyright 2026 AsterSQL.
use crate::StoreError;

/// A backend cancellation cannot prove a dispatched transaction was rolled back.
/// Preserve all other categories, including invalid requests and malformed responses.
pub(crate) fn dispatched_error(error: StoreError) -> StoreError {
    match error {
        StoreError::Cancelled => StoreError::Unavailable,
        other => other,
    }
}
