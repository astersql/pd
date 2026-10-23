// Copyright 2026 AsterSQL.

use crate::{EndpointError, LoadState, StorageEndpoint, protobuf};
use astersql_pd_core::{KeyRange, Peer, PeerRole, RegionEpoch, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::Base;
use prost::Message;

/// Region persistence errors, including explicit refusal of encrypted metadata.
#[derive(Debug, thiserror::Error)]
pub enum RegionMetaError {
    #[error(transparent)]
    Endpoint(#[from] EndpointError),
    #[error("region encryption metadata requires an encryption key manager")]
    Encrypted,
}
/// Load failure with Go's existence flag, including malformed nonempty values.
#[derive(Debug, thiserror::Error)]
#[error("region load failed (found={found}): {source}")]
pub struct RegionLoadError {
    pub found: bool,
    #[source]
    pub source: RegionMetaError,
}

#[derive(prost::Message)]
struct WireRegion {
    #[prost(uint64, tag = "1")]
    id: u64,
    #[prost(bytes = "vec", tag = "2")]
    start_key: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    end_key: Vec<u8>,
    #[prost(message, optional, tag = "4")]
    epoch: Option<WireEpoch>,
    #[prost(message, repeated, tag = "5")]
    peers: Vec<WirePeer>,
    #[prost(message, optional, tag = "6")]
    encryption: Option<WireEncryption>,
    #[prost(bool, tag = "7")]
    is_in_flashback: bool,
    #[prost(uint64, tag = "8")]
    flashback_start_ts: u64,
}
#[derive(prost::Message)]
struct WireEpoch {
    #[prost(uint64, tag = "1")]
    conf_ver: u64,
    #[prost(uint64, tag = "2")]
    version: u64,
}
#[derive(prost::Message)]
struct WirePeer {
    #[prost(uint64, tag = "1")]
    id: u64,
    #[prost(uint64, tag = "2")]
    store_id: u64,
    #[prost(int32, tag = "3")]
    role: i32,
    #[prost(bool, tag = "4")]
    is_witness: bool,
}
#[derive(prost::Message)]
struct WireEncryption {
    #[prost(uint64, tag = "1")]
    key_id: u64,
    #[prost(bytes = "vec", tag = "2")]
    iv: Vec<u8>,
}
impl From<&RegionMeta> for WireRegion {
    fn from(meta: &RegionMeta) -> Self {
        Self {
            id: meta.id,
            start_key: meta.key_range.start_key.clone(),
            end_key: meta.key_range.end_key.clone(),
            epoch: meta.epoch.map(|v| WireEpoch {
                conf_ver: v.conf_ver,
                version: v.version,
            }),
            peers: meta
                .peers
                .iter()
                .map(|p| WirePeer {
                    id: p.id,
                    store_id: p.store_id,
                    is_witness: p.is_witness,
                    role: match p.role {
                        PeerRole::Voter => 0,
                        PeerRole::Learner => 1,
                        PeerRole::IncomingVoter => 2,
                        PeerRole::DemotingVoter => 3,
                        PeerRole::Unknown(v) => v,
                    },
                })
                .collect(),
            encryption: None,
            is_in_flashback: meta.is_in_flashback,
            flashback_start_ts: meta.flashback_start_ts,
        }
    }
}
impl TryFrom<WireRegion> for RegionMeta {
    type Error = RegionMetaError;
    fn try_from(w: WireRegion) -> Result<Self, Self::Error> {
        if w.encryption.is_some() {
            return Err(RegionMetaError::Encrypted);
        }
        Ok(Self {
            id: w.id,
            key_range: KeyRange {
                start_key: w.start_key,
                end_key: w.end_key,
            },
            epoch: w.epoch.map(|v| RegionEpoch {
                conf_ver: v.conf_ver,
                version: v.version,
            }),
            peers: w
                .peers
                .into_iter()
                .map(|p| Peer {
                    id: p.id,
                    store_id: p.store_id,
                    is_witness: p.is_witness,
                    role: match p.role {
                        0 => PeerRole::Voter,
                        1 => PeerRole::Learner,
                        2 => PeerRole::IncomingVoter,
                        3 => PeerRole::DemotingVoter,
                        v => PeerRole::Unknown(v),
                    },
                })
                .collect(),
            encryption_meta: None,
            is_in_flashback: w.is_in_flashback,
            flashback_start_ts: w.flashback_start_ts,
        })
    }
}
impl StorageEndpoint {
    /// Load one region; empty/missing is absent, corrupt/encrypted data is found.
    pub async fn load_region(
        &self,
        paths: &ClusterPaths,
        id: u64,
    ) -> Result<Option<RegionMeta>, RegionLoadError> {
        let state = self
            .load(paths.region_path(id).as_bytes())
            .await
            .map_err(|e| RegionLoadError {
                found: false,
                source: e.into(),
            })?;
        match state {
            LoadState::Missing | LoadState::Empty => Ok(None),
            LoadState::Value(bytes) => {
                let result = WireRegion::decode(bytes.as_slice())
                    .map_err(EndpointError::ProtoUnmarshal)
                    .map_err(RegionMetaError::from)
                    .and_then(RegionMeta::try_from);
                result.map(Some).map_err(|source| RegionLoadError {
                    found: true,
                    source,
                })
            }
        }
    }
    /// Save unencrypted metadata, rejecting encrypted inputs before writing.
    pub async fn save_region(
        &self,
        paths: &ClusterPaths,
        meta: &RegionMeta,
    ) -> Result<(), RegionMetaError> {
        if meta.encryption_meta.is_some() {
            return Err(RegionMetaError::Encrypted);
        }
        protobuf::save(
            self,
            paths.region_path(meta.id).as_bytes(),
            &WireRegion::from(meta),
        )
        .await?;
        Ok(())
    }
    /// Delete only this region's metadata; missing keys are harmless.
    pub async fn delete_region(
        &self,
        paths: &ClusterPaths,
        meta: &RegionMeta,
    ) -> Result<(), RegionMetaError> {
        Base::new(self.store.as_ref())
            .remove(paths.region_path(meta.id).as_bytes())
            .await
            .map_err(EndpointError::from)?;
        Ok(())
    }
}
