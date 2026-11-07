// Copyright 2026 AsterSQL.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use openraft::error::{InstallSnapshotError, RPCError, RaftError, RemoteError, Unreachable};
use openraft::network::RPCOption;
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::{
    BasicNode, Config, Raft, RaftLogReader, RaftNetwork, RaftNetworkFactory, SnapshotPolicy,
};

use super::candidate::Candidate;
use super::candidate_power_vfs_test::PowerVfs;
use super::candidate_raft::{PutPair, RaftDisk, Type};

type Nodes = Arc<RwLock<BTreeMap<u64, Raft<Type>>>>;
type RpcResult<T, E = openraft::error::Infallible> =
    Result<T, RPCError<u64, BasicNode, RaftError<u64, E>>>;

#[derive(Clone, Default)]
struct Network(Nodes);

struct Peer(Network, u64);

impl Peer {
    fn target<E>(&self) -> RpcResult<Raft<Type>, E>
    where
        E: std::error::Error,
    {
        self.0
            .0
            .read()
            .unwrap()
            .get(&self.1)
            .cloned()
            .ok_or_else(|| Unreachable::new(&io::Error::other("isolated peer")).into())
    }
}

impl RaftNetworkFactory<Type> for Network {
    type Network = Peer;

    async fn new_client(&mut self, target: u64, _: &BasicNode) -> Peer {
        Peer(self.clone(), target)
    }
}

impl RaftNetwork<Type> for Peer {
    async fn append_entries(
        &mut self,
        req: AppendEntriesRequest<Type>,
        _: RPCOption,
    ) -> RpcResult<AppendEntriesResponse<u64>> {
        self.target()?
            .append_entries(req)
            .await
            .map_err(|e| RemoteError::new(self.1, e).into())
    }

    async fn vote(&mut self, req: VoteRequest<u64>, _: RPCOption) -> RpcResult<VoteResponse<u64>> {
        self.target()?
            .vote(req)
            .await
            .map_err(|e| RemoteError::new(self.1, e).into())
    }

    async fn install_snapshot(
        &mut self,
        req: InstallSnapshotRequest<Type>,
        _: RPCOption,
    ) -> RpcResult<InstallSnapshotResponse<u64>, InstallSnapshotError> {
        self.target()?
            .install_snapshot(req)
            .await
            .map_err(|e| RemoteError::new(self.1, e).into())
    }
}

async fn start(
    path: &Path,
    network: &Network,
    id: u64,
    vfs: Option<&str>,
) -> (Raft<Type>, RaftDisk) {
    let disk = RaftDisk::new(Candidate::open_using(path, vfs).unwrap());
    let config = Config {
        heartbeat_interval: 30,
        election_timeout_min: 120,
        election_timeout_max: 240,
        snapshot_policy: SnapshotPolicy::Never,
        ..Default::default()
    }
    .validate()
    .unwrap();
    let raft = Raft::new(
        id,
        Arc::new(config),
        network.clone(),
        disk.clone(),
        disk.clone(),
    )
    .await
    .unwrap();
    network.0.write().unwrap().insert(id, raft.clone());
    (raft, disk)
}

async fn leader(nodes: &BTreeMap<u64, (Raft<Type>, RaftDisk)>) -> u64 {
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            for (&id, (node, _)) in nodes {
                if node.current_leader().await == Some(id)
                    && node.ensure_linearizable().await.is_ok()
                {
                    return id;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn theme10_candidate_quorum_reopen() {
    quorum_reopen(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn theme10_candidate_power_quorum() {
    quorum_reopen(true).await;
}

async fn quorum_reopen(power_cut: bool) {
    let dir = tempfile::tempdir().unwrap();
    let vfs_name = "pd-candidate-power-test";
    let vfs = power_cut.then(|| PowerVfs::new(dir.path(), vfs_name));
    let paths = (1..=3)
        .map(|id| {
            let path = dir.path().join(format!("node-{id}"));
            fs::create_dir(&path).unwrap();
            (id, path.join("meta.db"))
        })
        .collect::<BTreeMap<_, _>>();
    let network = Network::default();
    let mut nodes = BTreeMap::new();
    for (&id, path) in &paths {
        nodes.insert(
            id,
            start(path, &network, id, power_cut.then_some(vfs_name)).await,
        );
    }
    let voters = paths
        .keys()
        .map(|&id| (id, BasicNode::new(id.to_string())))
        .collect::<BTreeMap<_, _>>();
    nodes[&1].0.initialize(voters).await.unwrap();
    let primary = leader(&nodes).await;
    for (raft, _) in nodes.values() {
        raft.wait(Some(Duration::from_secs(5)))
            .applied_index_at_least(Some(0), "bootstrap membership")
            .await
            .unwrap();
    }
    let stale = *paths.keys().find(|&&id| id != primary).unwrap();
    network.0.write().unwrap().remove(&stale);

    let pair = [
        (b"a".to_vec(), b"one".to_vec()),
        (b"b".to_vec(), b"two".to_vec()),
    ];
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        nodes[&primary].0.client_write(PutPair(pair.clone())),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.data, 1);
    let ack = dir.path().join("client.json");
    let mut log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&ack)
        .unwrap();
    serde_json::to_writer(&mut log, &(response.log_id, response.data, &pair)).unwrap();
    log.flush().unwrap();
    log.sync_all().unwrap();
    drop(log);

    let mut restart_paths = paths.clone();
    if let Some(vfs) = vfs {
        for (&id, path) in &paths {
            let image = dir.path().join(format!("recovered-{id}"));
            fs::create_dir(&image).unwrap();
            let syncs = vfs.cut(path.parent().unwrap(), &image).unwrap();
            assert!(
                syncs
                    .iter()
                    .any(|path| path.ends_with(format!("node-{id}/meta.db-wal")))
            );
            restart_paths.insert(id, image.join("meta.db"));
        }
        for (&id, path) in &restart_paths {
            let mut disk = RaftDisk::new(Candidate::open(path).unwrap());
            let entries = disk
                .try_get_log_entries(response.log_id.index..=response.log_id.index)
                .await
                .unwrap();
            if id == stale {
                assert!(
                    entries.is_empty(),
                    "isolated voter unexpectedly has the committed log"
                );
            } else {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].log_id, response.log_id);
            }
        }
    }
    for (raft, _) in nodes.values() {
        raft.shutdown().await.unwrap();
    }
    network.0.write().unwrap().clear();
    drop(nodes);
    let reopened = Network::default();
    let mut survivors = BTreeMap::new();
    for (&id, path) in &restart_paths {
        if id != primary {
            survivors.insert(id, start(path, &reopened, id, None).await);
        }
    }
    let elected = leader(&survivors).await;
    let (_, disk) = &survivors[&elected];
    let (log_id, revision, values): (openraft::LogId<u64>, u64, [(Vec<u8>, Vec<u8>); 2]) =
        serde_json::from_reader(fs::File::open(ack).unwrap()).unwrap();
    assert_eq!(log_id, response.log_id);
    assert_eq!(revision, response.data);
    for (key, expected) in values {
        assert_eq!(disk.value(&key).unwrap(), Some(expected));
    }
    println!(
        "power_cut={power_cut} old_leader={primary} stale_voter={stale} recovered_leader={elected} log={log_id} revision={revision} recovered_keys=2"
    );
    for (raft, _) in survivors.values() {
        raft.shutdown().await.unwrap();
    }
    reopened.0.write().unwrap().clear();
}
