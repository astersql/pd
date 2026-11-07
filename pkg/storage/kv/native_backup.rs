// Copyright 2026 AsterSQL.
//! Versioned native recovery archive. Raft membership, votes and peer credentials
//! are outside the archive. Restoring starts a new, isolated consensus identity.

use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use openraft::error::{Infallible, InstallSnapshotError, RPCError, RaftError, Unreachable};
use openraft::network::RPCOption;
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::{BasicNode, Config, RaftNetwork, RaftNetworkFactory, SnapshotPolicy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::native_state::{NativeCommand, NativeReply};
use crate::{Entry, LeaseId, NativeMetaStore, NativeRaft, Revision};

const FORMAT: &str = "astersql-pd-native-backup/1";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Payload used by the single bootstrap log entry on an offline restore.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecoveryImage {
    pub(super) source_cluster: u64,
    pub(super) revision: Revision,
    pub(super) next_lease: LeaseId,
    pub(super) entries: Vec<Entry>,
}

#[derive(Deserialize, Serialize)]
struct Archive {
    format: String,
    sha256: [u8; 32],
    image: RecoveryImage,
}

fn checksum(image: &RecoveryImage) -> Result<[u8; 32]> {
    Ok(Sha256::digest(serde_json::to_vec(image)?).into())
}

pub(super) fn encode(image: RecoveryImage) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&Archive {
        format: FORMAT.to_owned(),
        sha256: checksum(&image)?,
        image,
    })?)
}

pub(super) fn publish(bytes: &[u8], path: &Path) -> Result<()> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut pending = tempfile::NamedTempFile::new_in(directory)?;
    pending.write_all(bytes)?;
    pending.as_file().sync_all()?;
    pending.persist_noclobber(path)?;
    #[cfg(unix)]
    std::fs::File::open(directory)?.sync_all()?;
    Ok(())
}

fn decode(bytes: &[u8]) -> Result<RecoveryImage> {
    let archive: Archive = serde_json::from_slice(bytes)?;
    if archive.format != FORMAT || archive.sha256 != checksum(&archive.image)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid native archive format or checksum",
        )
        .into());
    }
    Ok(archive.image)
}

/// Restore a verified archive into a new directory as a one-voter cluster.
/// The destination ID must differ from the source consensus ID. The offline
/// operation opens no sockets and never contacts the source.
pub async fn restore_native_backup(
    bytes: &[u8],
    directory: &Path,
    cluster: u64,
    node: u64,
) -> Result<()> {
    let image = decode(bytes)?;
    if cluster == 0 || node == 0 || cluster == image.source_cluster || image.source_cluster == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid or reused native identity",
        )
        .into());
    }
    image.validate()?;
    std::fs::create_dir(directory)?;
    let store = open_isolated(directory, cluster, node).await?;
    let result = async {
        store
            .consensus()
            .initialize(std::collections::BTreeSet::from([node]))
            .await?;
        store
            .consensus()
            .wait(Some(Duration::from_secs(5)))
            .current_leader(node, "offline restore election")
            .await?;
        match store
            .consensus()
            .client_write(NativeCommand::Restore(image))
            .await?
            .data
        {
            NativeReply::Administrative(result) => Ok(result?),
            _ => Err(io::Error::other("unexpected native recovery response").into()),
        }
    }
    .await;
    let closed = store.close().await;
    result.and(closed)?;
    #[cfg(unix)]
    {
        let parent = directory
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

/// Open one native node with networking disabled, for isolated tools and tests.
pub async fn open_isolated(directory: &Path, cluster: u64, node: u64) -> Result<NativeMetaStore> {
    let config = Arc::new(
        Config {
            heartbeat_interval: 30,
            election_timeout_min: 120,
            election_timeout_max: 240,
            snapshot_policy: SnapshotPolicy::Never,
            ..Config::default()
        }
        .validate()?,
    );
    NativeMetaStore::open(directory, cluster, node, config, OfflineNetwork).await
}

#[derive(Clone, Copy)]
struct OfflineNetwork;

type OfflineRpc<T, E = Infallible> =
    std::result::Result<T, RPCError<u64, BasicNode, RaftError<u64, E>>>;

fn offline<T, E: std::error::Error>() -> OfflineRpc<T, E> {
    Err(Unreachable::new(&io::Error::other("offline native recovery")).into())
}

impl RaftNetworkFactory<NativeRaft> for OfflineNetwork {
    type Network = Self;
    async fn new_client(&mut self, _: u64, _: &BasicNode) -> Self {
        *self
    }
}

impl RaftNetwork<NativeRaft> for OfflineNetwork {
    async fn append_entries(
        &mut self,
        _: AppendEntriesRequest<NativeRaft>,
        _: RPCOption,
    ) -> OfflineRpc<AppendEntriesResponse<u64>> {
        offline()
    }

    async fn vote(&mut self, _: VoteRequest<u64>, _: RPCOption) -> OfflineRpc<VoteResponse<u64>> {
        offline()
    }

    async fn install_snapshot(
        &mut self,
        _: InstallSnapshotRequest<NativeRaft>,
        _: RPCOption,
    ) -> OfflineRpc<InstallSnapshotResponse<u64>, InstallSnapshotError> {
        offline()
    }
}
