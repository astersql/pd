// Copyright 2026 AsterSQL.

use crate::{EndpointError, LoadState, StorageEndpoint};
use astersql_pd_keypath::ClusterPaths;
use serde::{Serialize, de::DeserializeOwned};

impl StorageEndpoint {
    /// Decode a fresh persistence model. None means missing or empty bytes;
    /// nonempty JSON null is found and yields the model's zero/default value.
    /// The caller owns Go field mappings and nullable fields; this adapter does
    /// not supply server configuration defaults or validate business options.
    pub async fn load_config<T: DeserializeOwned + Default>(
        &self,
        paths: &ClusterPaths,
    ) -> Result<Option<T>, EndpointError> {
        self.load_config_model(paths.config_path().as_bytes()).await
    }

    /// Encode the caller's configuration model before overwriting its cluster key.
    /// Serialization failure leaves the stored value intact. No lease is attached.
    pub async fn save_config<T: Serialize + ?Sized>(
        &self,
        paths: &ClusterPaths,
        config: &T,
    ) -> Result<(), EndpointError> {
        self.save_json(paths.config_path().as_bytes(), config).await
    }

    /// Decode a fresh middleware model, with the same presence semantics as load_config.
    pub async fn load_service_middleware_config<T: DeserializeOwned + Default>(
        &self,
        paths: &ClusterPaths,
    ) -> Result<Option<T>, EndpointError> {
        self.load_config_model(paths.service_middleware_path().as_bytes())
            .await
    }

    /// Save middleware configuration independently of ordinary configuration.
    pub async fn save_service_middleware_config<T: Serialize + ?Sized>(
        &self,
        paths: &ClusterPaths,
        config: &T,
    ) -> Result<(), EndpointError> {
        self.save_json(paths.service_middleware_path().as_bytes(), config)
            .await
    }

    async fn load_config_model<T: DeserializeOwned + Default>(
        &self,
        key: &[u8],
    ) -> Result<Option<T>, EndpointError> {
        match self.load(key).await? {
            LoadState::Missing | LoadState::Empty => Ok(None),
            LoadState::Value(bytes) => serde_json::from_slice::<Option<T>>(&bytes)
                .map(|model| Some(model.unwrap_or_default()))
                .map_err(EndpointError::JsonUnmarshal),
        }
    }
}
