// Copyright 2026 AsterSQL.
//! The same bounded two-key linearizability check against replay and real etcd.

use std::sync::atomic::{AtomicU64, Ordering};

use etcd_client::{Client, Txn, TxnOp, TxnOpResponse};
use tokio::sync::Barrier;
use tokio::task::JoinSet;

use super::{
    BTreeMap, BasicNode, Duration, Network, PutPair, Raft, Replay, Type, leader, shutdown,
    start_replay,
};
use std::sync::Arc;

const KEYS: [&[u8]; 2] = [b"/history/a", b"/history/b"];
const WORKERS: usize = 4;
const WRITES_PER_WORKER: usize = 8;

#[derive(Clone)]
enum Backend {
    Replay(Raft<Type>, Replay),
    Etcd(Client, i64),
}

impl Backend {
    async fn write(&mut self, value: Vec<u8>) -> u64 {
        match self {
            Self::Replay(raft, _) => {
                raft.client_write(PutPair(KEYS.map(|key| (key.to_vec(), value.clone()))))
                    .await
                    .unwrap()
                    .data
            }
            Self::Etcd(client, initial) => {
                let response = client
                    .txn(Txn::new().and_then(KEYS.map(|key| TxnOp::put(key, value.clone(), None))))
                    .await
                    .unwrap();
                assert!(response.succeeded());
                u64::try_from(response.header().unwrap().revision() - *initial).unwrap()
            }
        }
    }

    async fn read(&mut self) -> (u64, [Option<Vec<u8>>; 2]) {
        match self {
            Self::Replay(raft, replay) => {
                raft.ensure_linearizable().await.unwrap();
                replay.read_pair(KEYS).unwrap()
            }
            Self::Etcd(client, initial) => {
                let response = client
                    .txn(Txn::new().and_then(KEYS.map(|key| TxnOp::get(key, None))))
                    .await
                    .unwrap();
                let revision =
                    u64::try_from(response.header().unwrap().revision() - *initial).unwrap();
                let results = response.op_responses();
                let [TxnOpResponse::Get(a), TxnOpResponse::Get(b)] = results.as_slice() else {
                    panic!("expected a pair of reads");
                };
                (
                    revision,
                    [a, b].map(|r| r.kvs().first().map(|kv| kv.value().to_vec())),
                )
            }
        }
    }
}

struct Observation {
    start: u64,
    end: u64,
    revision: u64,
    result: Observed,
}

enum Observed {
    Write(Vec<u8>),
    Read([Option<Vec<u8>>; 2]),
}

async fn check(backend: Backend) {
    let mut tasks = JoinSet::new();
    let barrier = Arc::new(Barrier::new(WORKERS));
    let clock = Arc::new(AtomicU64::new(0));
    for worker in 0..WORKERS {
        let mut client = backend.clone();
        let barrier = barrier.clone();
        let clock = clock.clone();
        tasks.spawn(async move {
            let tick = || clock.fetch_add(1, Ordering::SeqCst);
            let mut observations = Vec::new();
            barrier.wait().await;
            for i in 0..WRITES_PER_WORKER {
                let value = format!("{worker}:{i}").into_bytes();
                let start = tick();
                let revision = client.write(value.clone()).await;
                observations.push(Observation {
                    start,
                    end: tick(),
                    revision,
                    result: Observed::Write(value),
                });
                let start = tick();
                let (revision, values) = client.read().await;
                observations.push(Observation {
                    start,
                    end: tick(),
                    revision,
                    result: Observed::Read(values),
                });
            }
            observations
        });
    }
    let mut observations = Vec::new();
    while let Some(task) = tasks.join_next().await {
        observations.extend(task.unwrap());
    }
    let committed: BTreeMap<_, _> = observations
        .iter()
        .filter_map(|event| match &event.result {
            Observed::Write(bytes) => Some((event.revision, bytes)),
            Observed::Read(_) => None,
        })
        .collect();
    let count = (WORKERS * WRITES_PER_WORKER) as u64;
    assert!(committed.keys().copied().eq(1..=count));
    for first in &observations {
        if let Observed::Read(values) = &first.result {
            let expected = committed.get(&first.revision).copied();
            assert_eq!(values.each_ref().map(Option::as_ref), [expected, expected]);
        }
        for next in observations.iter().filter(|next| first.end < next.start) {
            assert!(first.revision <= next.revision);
            if matches!(next.result, Observed::Write(_)) {
                assert!(first.revision < next.revision);
            }
        }
    }
    println!("history writes={count} reads={count} concurrent_clients={WORKERS}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn theme10_candidate_history_matches_real_etcd() {
    let timeout = Duration::from_secs(15);
    let root = tempfile::tempdir().unwrap();
    let network = Network::default();
    let mut nodes = BTreeMap::new();
    for id in 1..=3 {
        nodes.insert(
            id,
            start_replay(
                &root.path().join(format!("node{id}.db")),
                &network,
                id,
                None,
            )
            .await,
        );
    }
    let voters = nodes
        .keys()
        .map(|&id| (id, BasicNode::new(id.to_string())))
        .collect::<BTreeMap<_, _>>();
    nodes[&1].0.initialize(voters).await.unwrap();
    let primary = leader(&nodes).await;
    let (raft, replay) = nodes[&primary].clone();
    tokio::time::timeout(timeout, check(Backend::Replay(raft, replay)))
        .await
        .unwrap();
    shutdown(&network, nodes).await;

    let fixture = crate::fixture::Fixture::start();
    let mut client = Client::connect([fixture.endpoint.as_str()], None)
        .await
        .unwrap();
    let initial = client
        .get(KEYS[0], None)
        .await
        .unwrap()
        .header()
        .unwrap()
        .revision();
    tokio::time::timeout(timeout, check(Backend::Etcd(client, initial)))
        .await
        .unwrap();
}
