// Copyright 2026 AsterSQL.
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use openraft::error::{
    Infallible, InstallSnapshotError, RPCError, RaftError, RemoteError, Unreachable,
};
use openraft::network::RPCOption;
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::{BasicNode, Config, Raft, RaftNetwork, RaftNetworkFactory};
use tempfile::TempDir;

use crate::*;

#[derive(Clone, Default)]
pub(super) struct Network(Arc<RwLock<BTreeMap<u64, Raft<NativeRaft>>>>, u64);

type RpcResult<T, E = Infallible> =
    std::result::Result<T, RPCError<u64, BasicNode, RaftError<u64, E>>>;

pub(super) struct Peer(Network, u64);

impl Peer {
    fn target<E: std::error::Error>(&self) -> RpcResult<Raft<NativeRaft>, E> {
        let routes = self.0.0.read().unwrap();
        if routes.contains_key(&self.0.1) {
            if let Some(raft) = routes.get(&self.1) {
                return Ok(raft.clone());
            }
        }
        Err(Unreachable::new(&io::Error::other("isolated test peer")).into())
    }
}

impl RaftNetworkFactory<NativeRaft> for Network {
    type Network = Peer;

    async fn new_client(&mut self, target: u64, _: &BasicNode) -> Peer {
        Peer(self.clone(), target)
    }
}

impl RaftNetwork<NativeRaft> for Peer {
    async fn append_entries(
        &mut self,
        req: AppendEntriesRequest<NativeRaft>,
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
        req: InstallSnapshotRequest<NativeRaft>,
        _: RPCOption,
    ) -> RpcResult<InstallSnapshotResponse<u64>, InstallSnapshotError> {
        self.target()?
            .install_snapshot(req)
            .await
            .map_err(|e| RemoteError::new(self.1, e).into())
    }
}

pub(super) fn config() -> Arc<Config> {
    Arc::new(
        Config {
            heartbeat_interval: 30,
            election_timeout_min: 120,
            election_timeout_max: 240,
            snapshot_policy: openraft::SnapshotPolicy::LogsSinceLast(200),
            ..Config::default()
        }
        .validate()
        .unwrap(),
    )
}

impl Network {
    pub(super) async fn open(&self, dir: &Path, cluster: u64, id: u64) -> NativeMetaStore {
        let store = NativeMetaStore::open(dir, cluster, id, config(), Self(self.0.clone(), id))
            .await
            .unwrap();
        self.connect(id, &store);
        store
    }

    pub(super) fn connect(&self, id: u64, store: &NativeMetaStore) {
        self.0
            .write()
            .unwrap()
            .insert(id, store.consensus().clone());
    }

    pub(super) fn disconnect(&self, id: u64) {
        self.0.write().unwrap().remove(&id);
    }

    pub(super) async fn close(&self, id: u64, store: NativeMetaStore) {
        self.disconnect(id);
        store.close().await.unwrap();
    }
}

pub(super) async fn leader<'a>(
    nodes: impl IntoIterator<Item = &'a NativeMetaStore> + Clone,
) -> &'a NativeMetaStore {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            for node in nodes.clone() {
                if node.consensus().metrics().borrow().state == openraft::ServerState::Leader
                    && tokio::time::timeout(
                        Duration::from_millis(250),
                        node.consensus().ensure_linearizable(),
                    )
                    .await
                    .is_ok_and(|result| result.is_ok())
                {
                    return node;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("native leader must be available")
}

pub(super) async fn standalone(dir: &Path) -> (Network, NativeMetaStore) {
    let network = Network::default();
    let node = network.open(dir, 72, 1).await;
    node.consensus()
        .initialize(std::collections::BTreeSet::from([1]))
        .await
        .unwrap();
    leader([&node]).await;
    (network, node)
}

#[tokio::test]
async fn theme10_native_open() {
    let directory = TempDir::new().unwrap();
    let path = directory.path();
    let (network, store) = standalone(path).await;
    let revision = crate::native_contract_test::contract::put(&store, b"persist", b"v")
        .await
        .revision;
    assert!(
        NativeMetaStore::open(path, 72, 1, config(), Network::default())
            .await
            .is_err()
    );
    network.close(1, store).await;
    for (cluster, node) in [(73, 1), (72, 2)] {
        assert!(
            NativeMetaStore::open(path, cluster, node, config(), Network::default())
                .await
                .is_err()
        );
    }
    let store = network.open(path, 72, 1).await;
    leader([&store]).await;
    let read = crate::native_contract_test::contract::read(&store, b"persist").await;
    assert_eq!(read.revision, revision);
    assert_eq!(read.entries[0].value, b"v");
    network.close(1, store).await;
    let db = rusqlite::Connection::open(path.join("metadata.sqlite")).unwrap();
    let payload = serde_json::to_vec(&("astersql-pd-native", 2_u32, 72_u64, 1_u64)).unwrap();
    db.execute(
        "UPDATE state SET payload=?1 WHERE name='native_identity'",
        [payload],
    )
    .unwrap();
    drop(db);
    assert!(
        NativeMetaStore::open(path, 72, 1, config(), Network::default())
            .await
            .is_err()
    );
}
