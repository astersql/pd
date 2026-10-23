// Copyright 2026 AsterSQL.

use crate::{EndpointError, LoadState, StorageEndpoint};
use astersql_pd_keypath::ClusterPaths;
use serde::{Serialize, de::DeserializeOwned};

impl StorageEndpoint {
    /// Decode a fresh status model for one mode. Missing and empty bytes are
    /// absent; JSON null is present and yields the model's default value.
    /// The caller owns Go field mappings, nullable fields and custom codecs.
    pub async fn load_replication_status<T: DeserializeOwned + Default>(
        &self,
        paths: &ClusterPaths,
        mode: &str,
    ) -> Result<Option<T>, EndpointError> {
        match self
            .load(paths.replication_mode_path(mode).as_bytes())
            .await?
        {
            LoadState::Missing | LoadState::Empty => Ok(None),
            LoadState::Value(bytes) => serde_json::from_slice::<Option<T>>(&bytes)
                .map(|status| Some(status.unwrap_or_default()))
                .map_err(EndpointError::JsonUnmarshal),
        }
    }

    /// Encode before overwriting this mode's key without a lease. Serialization
    /// failure preserves the previous status; mode names are used verbatim.
    pub async fn save_replication_status<T: Serialize + ?Sized>(
        &self,
        paths: &ClusterPaths,
        mode: &str,
        status: &T,
    ) -> Result<(), EndpointError> {
        self.save_json(paths.replication_mode_path(mode).as_bytes(), status)
            .await
    }
}
