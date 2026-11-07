// Copyright 2026 AsterSQL.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use openraft::error::{InstallSnapshotError, RPCError, RaftError, RemoteError, Unreachable};
use openraft::network::RPCOption;
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::storage::{RaftLogStorage, RaftStateMachine};
use openraft::{
    BasicNode, Config, Raft, RaftLogReader, RaftNetwork, RaftNetworkFactory, RaftSnapshotBuilder,
    SnapshotPolicy,
};

use super::candidate::Candidate;
use super::candidate_power_vfs_test::{Fault, PowerVfs};
use super::candidate_raft::{PutPair, RaftDisk, Type};

type Nodes = Arc<RwLock<BTreeMap<u64, Raft<Type>>>>;
type Replicas = BTreeMap<u64, (Raft<Type>, RaftDisk)>;
type Paths = BTreeMap<u64, PathBuf>;
type RpcResult<T, E = openraft::error::Infallible> =
    Result<T, RPCError<u64, BasicNode, RaftError<u64, E>>>;

#[derive(Clone, Default)]
struct Network(Nodes, Option<u64>);

struct Peer(Network, u64);

impl Peer {
    fn target<E>(&self) -> RpcResult<Raft<Type>, E>
    where
        E: std::error::Error,
    {
        let routes = self.0.0.read().unwrap();
        let isolated = || Unreachable::new(&io::Error::other("isolated peer")).into();
        if self.0.1.is_some_and(|id| !routes.contains_key(&id)) {
            return Err(isolated());
        }
        routes.get(&self.1).cloned().ok_or_else(isolated)
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
        max_in_snapshot_log_to_keep: 0,
        purge_batch_size: 1,
        ..Default::default()
    }
    .validate()
    .unwrap();
    let raft = Raft::new(
        id,
        Arc::new(config),
        Network(network.0.clone(), Some(id)),
        disk.clone(),
        disk.clone(),
    )
    .await
    .unwrap();
    network.0.write().unwrap().insert(id, raft.clone());
    (raft, disk)
}

async fn leader(nodes: &Replicas) -> u64 {
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

async fn boot(root: &Path, vfs: Option<&str>) -> (Paths, Network, Replicas, u64) {
    let paths = (1..=3)
        .map(|id| {
            let path = root.join(format!("node-{id}"));
            fs::create_dir(&path).unwrap();
            (id, path.join("meta.db"))
        })
        .collect::<BTreeMap<_, _>>();
    let network = Network::default();
    let mut nodes = BTreeMap::new();
    for (&id, path) in &paths {
        nodes.insert(id, start(path, &network, id, vfs).await);
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
    (paths, network, nodes, primary)
}

async fn shutdown(network: &Network, nodes: Replicas) {
    for (raft, _) in nodes.values() {
        raft.shutdown().await.unwrap();
    }
    network.0.write().unwrap().clear();
}

fn sample() -> PutPair {
    PutPair([
        (b"a".to_vec(), b"one".to_vec()),
        (b"b".to_vec(), b"two".to_vec()),
    ])
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
    let (paths, network, nodes, primary) = boot(dir.path(), power_cut.then_some(vfs_name)).await;
    let stale = *paths.keys().find(|&&id| id != primary).unwrap();
    network.0.write().unwrap().remove(&stale);

    let pair = sample().0;
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
    shutdown(&network, nodes).await;
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
    shutdown(&reopened, survivors).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn theme10_candidate_snapshot_catchup_and_membership() {
    let dir = tempfile::tempdir().unwrap();
    let (_, network, mut nodes, primary) = boot(dir.path(), None).await;
    let stale = *nodes.keys().find(|&&id| id != primary).unwrap();
    // This test exercises catch-up from the same leader after its log is purged.
    // Re-election after losing a leader is covered by the quorum reopen tests.
    nodes[&stale].0.runtime_config().elect(false);
    network.0.write().unwrap().remove(&stale);
    let raft = nodes[&primary].0.clone();
    let committed = tokio::time::timeout(Duration::from_secs(5), raft.client_write(sample()))
        .await
        .unwrap()
        .unwrap();
    raft.trigger().snapshot().await.unwrap();
    raft.wait(Some(Duration::from_secs(5)))
        .snapshot(committed.log_id, "durable snapshot")
        .await
        .unwrap();
    raft.trigger()
        .purge_log(committed.log_id.index)
        .await
        .unwrap();
    raft.wait(Some(Duration::from_secs(5)))
        .purged(Some(committed.log_id), "logs purged")
        .await
        .unwrap();
    let mut isolated = RaftDisk::new(Candidate::open(&dir.path().join("checksum.db")).unwrap());
    let snapshot = nodes[&primary]
        .1
        .clone()
        .get_current_snapshot()
        .await
        .unwrap()
        .unwrap();
    let mut damaged: serde_json::Value =
        serde_json::from_slice(snapshot.snapshot.get_ref()).unwrap();
    damaged["sha256"][0] = serde_json::json!(damaged["sha256"][0].as_u64().unwrap() ^ 1);
    let damaged = Box::new(io::Cursor::new(serde_json::to_vec(&damaged).unwrap()));
    assert!(
        isolated
            .install_snapshot(&snapshot.meta, damaged)
            .await
            .is_err()
    );
    assert_eq!(isolated.applied_state().await.unwrap().0, None);
    isolated
        .install_snapshot(&snapshot.meta, snapshot.snapshot)
        .await
        .unwrap();
    assert_eq!(
        isolated.applied_state().await.unwrap().0,
        Some(committed.log_id)
    );
    network
        .0
        .write()
        .unwrap()
        .insert(stale, nodes[&stale].0.clone());
    nodes[&stale]
        .0
        .wait(Some(Duration::from_secs(7)))
        .snapshot(committed.log_id, "follower installed snapshot")
        .await
        .unwrap();
    for (key, val) in sample().0 {
        assert_eq!(nodes[&stale].1.value(&key).unwrap(), Some(val));
    }
    let new_id = 4;
    nodes.insert(
        new_id,
        start(&dir.path().join("new.db"), &network, new_id, None).await,
    );
    tokio::time::timeout(
        Duration::from_secs(7),
        raft.add_learner(new_id, BasicNode::new("4"), true),
    )
    .await
    .unwrap()
    .unwrap();
    let voters: BTreeSet<_> = nodes.keys().copied().filter(|&id| id != stale).collect();
    tokio::time::timeout(
        Duration::from_secs(7),
        raft.change_membership(voters.clone(), false),
    )
    .await
    .unwrap()
    .unwrap();
    raft.wait(Some(Duration::from_secs(5)))
        .voter_ids(voters, "replacement voter")
        .await
        .unwrap();
    network.0.write().unwrap().remove(&stale);
    let committed_after = tokio::time::timeout(Duration::from_secs(5), raft.client_write(sample()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(committed_after.data, 2);
    nodes[&new_id]
        .0
        .wait(Some(Duration::from_secs(5)))
        .applied_index_at_least(
            Some(committed_after.log_id.index),
            "new voter applied transaction",
        )
        .await
        .unwrap();
    shutdown(&network, nodes).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn theme10_candidate_snapshot_atomic_reopen() {
    for (stage, fault, purge) in [
        ("write-interrupted", Some(Fault::ShortWrite), false),
        ("sync-interrupted", Some(Fault::Sync), false),
        ("published", None, false),
        ("published-and-purged", None, true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let name = format!("pd-candidate-snapshot-{stage}");
        let vfs = PowerVfs::new(dir.path(), &name);
        let (paths, network, nodes, primary) = boot(dir.path(), Some(&name)).await;
        let (raft, disk) = &nodes[&primary];
        let mut disk = disk.clone();
        let timeout = Duration::from_secs(5);
        let first = tokio::time::timeout(timeout, raft.client_write(sample()))
            .await
            .unwrap()
            .unwrap();
        let initial = disk.build_snapshot().await.unwrap();
        assert_eq!(initial.meta.last_log_id, Some(first.log_id));
        let next_values = PutPair([
            (b"a".to_vec(), b"updated".to_vec()),
            (b"b".to_vec(), b"changed".to_vec()),
        ]);
        let second = tokio::time::timeout(timeout, raft.client_write(next_values.clone()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!((first.data, second.data), (1, 2));

        let source = paths[&primary].parent().unwrap();
        if let Some(fault) = fault {
            vfs.reject_wal_io(source, fault);
            assert!(disk.build_snapshot().await.is_err());
            assert!(vfs.rejected_io() > 0);
        } else {
            assert_eq!(
                disk.build_snapshot().await.unwrap().meta.last_log_id,
                Some(second.log_id)
            );
            if purge {
                disk.purge(second.log_id).await.unwrap();
            }
        }

        let image = dir.path().join("recovered");
        fs::create_dir(&image).unwrap();
        vfs.cut(source, &image).unwrap();
        shutdown(&network, nodes).await;
        let mut recovered = RaftDisk::new(Candidate::open(&image.join("meta.db")).unwrap());
        let (applied, _) = recovered.applied_state().await.unwrap();
        assert_eq!(applied, Some(second.log_id));
        for (key, val) in &next_values.0 {
            assert_eq!(recovered.value(key).unwrap().as_ref(), Some(val));
        }
        let log = recovered.get_log_state().await.unwrap();
        assert_eq!(log.last_purged_log_id, purge.then_some(second.log_id));
        assert_eq!(log.last_log_id, Some(second.log_id));
        let durable = recovered.get_current_snapshot().await.unwrap().unwrap();
        let expected_snapshot = if fault.is_some() {
            first.log_id
        } else {
            second.log_id
        };
        assert_eq!(durable.meta.last_log_id, Some(expected_snapshot));
        let mut installed =
            RaftDisk::new(Candidate::open(&dir.path().join("installed.db")).unwrap());
        installed
            .install_snapshot(&durable.meta, durable.snapshot)
            .await
            .unwrap();
        assert_eq!(
            installed.applied_state().await.unwrap().0,
            Some(expected_snapshot)
        );
        let expected = if fault.is_some() {
            sample()
        } else {
            next_values
        };
        for (key, val) in expected.0 {
            assert_eq!(installed.value(&key).unwrap(), Some(val));
        }
        println!(
            "snapshot_stage={stage} recovered_snapshot={expected_snapshot} applied={} purged={purge}",
            second.log_id
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn theme10_candidate_io_failure_stops_acknowledgement() {
    for (name, fault) in [
        ("pd-candidate-sync-fault", Fault::Sync),
        ("pd-candidate-full-fault", Fault::Full),
        ("pd-candidate-short-write-fault", Fault::ShortWrite),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let vfs = PowerVfs::new(dir.path(), name);
        let (paths, network, nodes, primary) = boot(dir.path(), Some(name)).await;
        let raft = &nodes[&primary].0;
        let confirmed = tokio::time::timeout(Duration::from_secs(5), raft.client_write(sample()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(confirmed.data, 1);
        let source = paths[&primary].parent().unwrap();
        vfs.reject_wal_io(source, fault);
        let second = PutPair([
            (b"a".to_vec(), b"bad".to_vec()),
            (b"c".to_vec(), b"new".to_vec()),
        ]);
        let result = tokio::time::timeout(Duration::from_secs(5), raft.client_write(second))
            .await
            .unwrap();
        assert!(
            result.is_err(),
            "leader acknowledged a WAL fault: {fault:?}"
        );
        assert!(vfs.rejected_io() > 0);
        let recovered = dir.path().join("recovered");
        fs::create_dir(&recovered).unwrap();
        vfs.cut(source, &recovered).unwrap();
        shutdown(&network, nodes).await;
        let mut disk = RaftDisk::new(Candidate::open(&recovered.join("meta.db")).unwrap());
        let (applied, _) = disk.applied_state().await.unwrap();
        assert_eq!(applied, Some(confirmed.log_id));
        for (key, value) in sample().0 {
            assert_eq!(disk.value(&key).unwrap(), Some(value));
        }
        assert_eq!(disk.value(b"c").unwrap(), None);
        println!(
            "io_fault={fault:?} rejected={} retained_revision={}",
            vfs.rejected_io(),
            confirmed.data
        );
    }
}
