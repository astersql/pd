// Copyright 2026 AsterSQL.
//! Local classic PD using the isolated one-voter metadata backend.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use astersql_pd_runtime::net_address::Endpoint;
use astersql_pd_storage::MetaStore;
use astersql_pd_storage_kv::{NativeMetaStore, open_isolated};

use crate::classic_process::{Config, ProcessError, run_with_backend};

const NODE_ID: u64 = 1;

pub(crate) struct LocalNative {
    directory: PathBuf,
    cluster_id: u64,
}

impl LocalNative {
    pub(crate) const DATA_DIR_FLAG: &str = "--local-native-data-dir";
    pub(crate) const CLUSTER_ID_FLAG: &str = "--local-native-cluster-id";

    pub(crate) fn from_flags(
        config: &Config,
        flags: &BTreeMap<String, String>,
    ) -> Result<Option<Self>, ProcessError> {
        let directory = flags.get(Self::DATA_DIR_FLAG);
        let cluster_id = flags.get(Self::CLUSTER_ID_FLAG);
        if directory.is_none() && cluster_id.is_none() {
            return Ok(None);
        }
        let (Some(directory), Some(cluster_id)) = (directory, cluster_id) else {
            return Err("local native mode requires both data directory and cluster ID".into());
        };
        let cluster_id = cluster_id
            .parse::<u64>()
            .ok()
            .filter(|id| *id != 0)
            .ok_or("local native cluster ID must be a nonzero integer")?;
        if directory.is_empty() {
            return Err("local native data directory must not be empty".into());
        }
        if flags.contains_key("--etcd-endpoints")
            || flags.contains_key("--etcd-member-exclusive")
            || config.diagnostic_endpoint.is_some()
            || !config.join.is_empty()
            || !config.unsupported.is_empty()
            || std::env::var("PD_SERVICE_MODE").is_ok_and(|mode| mode.eq_ignore_ascii_case("api"))
        {
            return Err(
                "local native mode requires classic PD without etcd or join settings".into(),
            );
        }
        for raw in [
            &config.client,
            &config.peer,
            config.advertise_client.as_ref().unwrap_or(&config.client),
            config.advertise_peer.as_ref().unwrap_or(&config.peer),
        ] {
            let url = Endpoint::parse(raw)?.url();
            let loopback = url
                .split_once("://")
                .and_then(|(_, authority)| authority.parse::<SocketAddr>().ok())
                .is_some_and(|address| address.ip().is_loopback());
            if !loopback {
                return Err("local native mode requires literal loopback URLs with ports".into());
            }
        }
        Ok(Some(Self {
            directory: PathBuf::from(directory),
            cluster_id,
        }))
    }

    async fn open(&self) -> Result<NativeMetaStore, ProcessError> {
        match std::fs::read_dir(&self.directory) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    if !matches!(
                        entry.file_name().to_str(),
                        Some(
                            "metadata.sqlite"
                                | "metadata.sqlite-wal"
                                | "metadata.sqlite-shm"
                                | "metadata.sqlite-journal"
                                | "metadata.lock"
                        )
                    ) || !entry.file_type()?.is_file()
                    {
                        return Err("local native mode requires a dedicated data directory".into());
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let store = open_isolated(&self.directory, self.cluster_id, NODE_ID).await?;
        let initialized = async {
            let raft = store.consensus();
            if !raft.is_initialized().await? {
                raft.initialize(BTreeSet::from([NODE_ID])).await?;
            }
            let metrics = raft
                .wait(Some(Duration::from_secs(5)))
                .current_leader(NODE_ID, "local native voter")
                .await?;
            let membership = &metrics.membership_config;
            if !membership.voter_ids().eq([NODE_ID])
                || !membership.nodes().map(|(id, _)| *id).eq([NODE_ID])
            {
                return Err("local native mode requires exactly one voter and no peers".into());
            }
            Ok::<(), ProcessError>(())
        }
        .await;
        if let Err(error) = initialized {
            if store.close().await.is_err() {
                tracing::error!(target: "pd", "local native store cleanup failed");
            }
            return Err(error);
        }
        Ok(store)
    }

    pub(crate) async fn run(
        self,
        config: Config,
        shutdown: impl Future<Output = Result<(), std::io::Error>>,
        log_level: Option<Arc<dyn astersql_pd_logutil::LogLevelControl>>,
    ) -> Result<Vec<&'static str>, ProcessError> {
        let mut native = None;
        let result = run_with_backend(config, shutdown, log_level, |_| async {
            let store = Arc::new(self.open().await?);
            let shared = Arc::clone(&store) as Arc<dyn MetaStore>;
            native = Some(store);
            tracing::info!(target: "pd", "local native store opened");
            Ok(shared)
        })
        .await;
        let closed = async {
            if let Some(store) = native {
                Arc::try_unwrap(store)
                    .map_err(|_| "PD retained the local native store after shutdown")?
                    .close()
                    .await?;
                tracing::info!(target: "pd", "local native store closed");
            }
            Ok(())
        }
        .await;
        result.and_then(|events| closed.map(|()| events))
    }
}
