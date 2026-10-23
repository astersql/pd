// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_kv::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

async fn behavior(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let campaign = || {
        Leadership::campaign(
            store.clone(),
            cancel.clone(),
            42,
            b"member-seven".to_vec(),
            5,
        )
    };
    let leader = campaign().await.unwrap();
    assert_eq!(leader.cluster_id(), 42);
    assert!(leader.uses_store(&store));
    assert!(!leader.uses_store(&(Arc::new(MemoryStore::default()) as Arc<dyn MetaStore>)));
    assert!(matches!(campaign().await, Err(ElectionError::Contended)));
    let key = b"/pd/42/leader";
    let snapshot = store
        .read(Read::prefix(key, Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(snapshot.entries[0].value, b"member-seven");
    assert_eq!(snapshot.entries[0].lease, Some(leader.lease_id()));
    assert_eq!(snapshot.entries[0].mod_revision, leader.revision());
    leader.renew().await.unwrap();
    assert!(
        leader
            .transact(vec![], vec![Mutation::put(b"/application", b"ok")])
            .await
            .unwrap()
            .succeeded
    );
    assert!(
        !leader
            .transact(
                vec![Compare::CreateRevision(b"/application".to_vec(), 0)],
                vec![Mutation::put(b"/application", b"bad")]
            )
            .await
            .unwrap()
            .succeeded
    );
    for write in [
        Mutation::put(key, b"bad"),
        Mutation::Delete { key: key.to_vec() },
    ] {
        assert!(matches!(
            leader.transact(vec![], vec![write]).await,
            Err(ElectionError::InvalidArgument)
        ));
    }
    // Same value and lease, newer mod_revision: the captured term must be fenced.
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: key.to_vec(),
                value: b"member-seven".to_vec(),
                lease: Some(leader.lease_id()),
            }],
        )
        .await
        .unwrap();
    assert!(matches!(
        leader.validate().await,
        Err(ElectionError::NotLeader)
    ));
    assert!(matches!(
        leader
            .transact(vec![], vec![Mutation::put(b"/must-not-write", b"x")])
            .await,
        Err(ElectionError::NotLeader)
    ));
    leader.resign().await.unwrap();
    let replacement = campaign().await.unwrap();
    assert_ne!(leader.lease_id(), replacement.lease_id());
    assert!(replacement.revision() > leader.revision());
    leader.resign().await.unwrap();
    replacement.validate().await.unwrap();
    assert!(matches!(
        leader.renew().await,
        Err(ElectionError::NotLeader)
    ));
    // Same lease, changed value: even an otherwise valid lease is insufficient.
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: key.to_vec(),
                value: b"changed".to_vec(),
                lease: Some(replacement.lease_id()),
            }],
        )
        .await
        .unwrap();
    assert!(matches!(
        replacement.validate().await,
        Err(ElectionError::NotLeader)
    ));
    replacement.resign().await.unwrap();
    let final_term = campaign().await.unwrap();
    cancel.cancel();
    assert!(matches!(
        final_term.validate().await,
        Err(ElectionError::Cancelled)
    ));
    assert!(matches!(campaign().await, Err(ElectionError::Cancelled)));
    final_term.resign().await.unwrap();
    final_term.resign().await.unwrap();
    assert!(
        store
            .read(Read::prefix(key, Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(
        store
            .read(Read::prefix(b"/must-not-write", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
}

#[tokio::test]
async fn leadership_move_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn leadership_move_etcd() {
    let fixture = super::etcd_fixture::Fixture::start();
    behavior(Arc::new(fixture.store().await)).await;
}
