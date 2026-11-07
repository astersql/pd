// Copyright 2026 AsterSQL.
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use crate::native_contract_test::contract;
use crate::native_open_test::{Network, leader};
use crate::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn theme10_native_fencing_after_partition_and_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let directory = |id| root.path().join(format!("node-{id}"));
    let network = Network::default();
    let mut nodes = BTreeMap::new();
    for id in 1..=3 {
        nodes.insert(id, network.open(&directory(id), 72, id).await);
    }
    nodes[&1]
        .consensus()
        .initialize(BTreeSet::from([1, 2, 3]))
        .await
        .unwrap();
    let initial = leader(nodes.values()).await;
    let old_id = initial.consensus().metrics().borrow().id;
    let lease = initial.grant(5).await.unwrap();
    let key = b"leader";
    let attach = |id| Mutation::Put {
        key: key.to_vec(),
        value: b"same".to_vec(),
        lease: Some(id),
    };
    let first = initial
        .transact(vec![], vec![attach(lease.id)])
        .await
        .unwrap();
    network.disconnect(old_id);
    let new = leader(
        nodes
            .iter()
            .filter(|(id, _)| **id != old_id)
            .map(|(_, store)| store)
            .collect::<Vec<_>>(),
    )
    .await;
    let new_id = new.consensus().metrics().borrow().id;
    let (read, write, renewed) = tokio::join!(
        initial.read(Read::prefix(key, Consistency::Linearizable)),
        initial.transact(
            vec![Compare::Lease(key.to_vec(), lease.id)],
            vec![Mutation::put(b"forbidden", b"old")]
        ),
        new.keep_alive(lease.id),
    );
    assert_eq!(read.unwrap_err(), StoreError::Unavailable);
    assert_eq!(write.unwrap_err(), StoreError::Unavailable);
    assert_eq!(renewed.unwrap().id, lease.id);
    new.revoke(lease.id).await.unwrap();
    let next_lease = new.grant(60).await.unwrap();
    let second = new
        .transact(
            vec![Compare::NotExists(key.to_vec())],
            vec![attach(next_lease.id)],
        )
        .await
        .unwrap();
    assert!(second.succeeded);
    assert!(next_lease.id > lease.id);
    assert!(second.revision > first.revision);
    let compare = vec![
        Compare::Value(key.to_vec(), b"same".to_vec()),
        Compare::Lease(key.to_vec(), lease.id),
    ];
    assert!(
        !new.transact(compare, vec![Mutation::put(b"forbidden", b"new")])
            .await
            .unwrap()
            .succeeded
    );
    assert!(contract::read(new, b"forbidden").await.entries.is_empty());
    new.compact(second.revision).await.unwrap();
    network.connect(old_id, initial);

    new.consensus().trigger().snapshot().await.unwrap();
    let snapshot = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(snapshot) = new.consensus().get_snapshot().await.unwrap() {
                break snapshot;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let snapshot_index = snapshot.meta.last_log_id.unwrap().index;
    new.consensus()
        .trigger()
        .purge_log(snapshot_index)
        .await
        .unwrap();
    new.consensus()
        .wait(Some(Duration::from_secs(5)))
        .metrics(
            |metrics| metrics.purged.is_some_and(|id| id.index >= snapshot_index),
            "snapshot published and old log truncated",
        )
        .await
        .unwrap();
    let learner = network.open(&directory(4), 72, 4).await;
    new.consensus()
        .add_learner(4, openraft::BasicNode::default(), true)
        .await
        .unwrap();
    learner
        .consensus()
        .wait(Some(Duration::from_secs(5)))
        .metrics(
            |metrics| {
                metrics
                    .snapshot
                    .is_some_and(|id| id.index >= snapshot_index)
                    && metrics
                        .last_applied
                        .is_some_and(|id| id.index >= snapshot_index)
            },
            "learner installed and applied snapshot",
        )
        .await
        .unwrap();
    let locally = learner
        .read(Read::prefix(key, Consistency::Serializable))
        .await
        .unwrap();
    assert_eq!(locally.entries[0].lease, Some(next_lease.id));
    let mut compacted = Read::prefix(key, Consistency::Serializable);
    compacted.revision = Some(first.revision);
    assert_eq!(
        learner.read(compacted).await.unwrap_err(),
        StoreError::Compacted {
            requested: first.revision,
            compact_revision: Some(second.revision)
        }
    );
    network.close(4, learner).await;
    let learner = network.open(&directory(4), 72, 4).await;
    assert_eq!(
        learner
            .read(Read::prefix(key, Consistency::Serializable))
            .await
            .unwrap()
            .entries[0]
            .lease,
        Some(next_lease.id)
    );
    network.close(4, learner).await;
    assert_ne!(new_id, old_id);
    for (id, store) in nodes {
        network.close(id, store).await;
    }
}
