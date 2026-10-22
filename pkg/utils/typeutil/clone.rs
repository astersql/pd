// Copyright 2026 AsterSQL.

use serde::{Serialize, de::DeserializeOwned};

/// Copies a value through its JSON serialization boundary into owned data.
///
/// Prefer native [`Clone`] for ordinary Rust values. Use this operation only
/// when the JSON representation defines the desired copy: skipped fields and
/// custom codecs follow their serde semantics. `Option::None` remains `None`.
/// Serialization and deserialization failures are returned to the caller;
/// no partial value or factory is exposed. This is not a protobuf codec.
pub fn deep_clone_json<T: Serialize + DeserializeOwned>(source: &T) -> serde_json::Result<T> {
    serde_json::from_slice(&serde_json::to_vec(source)?)
}
