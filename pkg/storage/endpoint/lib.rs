// Copyright 2026 AsterSQL.
//! Persistence adapters over the canonical backend-neutral MetaStore.
mod endpoint_codec;
/// Protobuf persistence adapter; protocol types remain outside domain interfaces.
pub use endpoint_codec::protobuf;
pub use endpoint_codec::{EndpointError, LoadState, StorageEndpoint};
#[cfg(test)]
mod endpoint_codec_test;
