// Copyright 2026 AsterSQL.
use astersql_pd_member::*;
use astersql_pd_storage_kv::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub async fn behavior(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(
        store.clone(),
        cancel.clone(),
        42,
        7,
        b"member-seven".to_vec(),
    )
    .await
    .unwrap();
    let again = Member::bootstrap(
        store.clone(),
        cancel.clone(),
        42,
        7,
        b"member-seven".to_vec(),
    )
    .await
    .unwrap();
    assert!(matches!(
        Member::bootstrap(store.clone(), cancel.clone(), 42, 7, b"changed".to_vec()).await,
        Err(MemberError::IdentityConflict)
    ));
    let leader = member.campaign(5).await.unwrap();
    assert!(matches!(
        again.campaign(5).await,
        Err(MemberError::Contended)
    ));
    let snapshot = store
        .read(Read::prefix(b"/pd/42/leader", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(snapshot.entries[0].lease, Some(leader.lease_id()));
    assert_eq!(snapshot.entries[0].mod_revision, leader.revision());
    leader.renew().await.unwrap();
    store.revoke(leader.lease_id()).await.unwrap();
    assert!(matches!(
        leader
            .transact(vec![], vec![Mutation::put(b"/must-not-write", b"x")])
            .await,
        Err(MemberError::NotLeader)
    ));
    let replacement = again.campaign(5).await.unwrap();
    assert_ne!(leader.lease_id(), replacement.lease_id());
    assert!(matches!(
        leader.transact(vec![], vec![]).await,
        Err(MemberError::NotLeader)
    ));
    assert!(
        store
            .read(Read::prefix(b"/must-not-write", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    eprintln!(
        "lease loss: old={} revision={}, replacement={} revision={}",
        leader.lease_id(),
        leader.revision(),
        replacement.lease_id(),
        replacement.revision()
    );
    replacement.resign().await.unwrap();
    cancel.cancel();
    assert!(matches!(
        again.campaign(5).await,
        Err(MemberError::Cancelled)
    ));
}
#[tokio::test]
async fn memory_bootstrap_and_lease_loss() {
    behavior(Arc::new(MemoryStore::default())).await;
}

pub async fn expiry(store: Arc<dyn MetaStore>) {
    let member = Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        43,
        1,
        b"expiry".to_vec(),
    )
    .await
    .unwrap();
    let leader = member.campaign(1).await.unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match leader.validate().await {
            Err(MemberError::NotLeader) => break,
            Ok(()) => {
                assert!(tokio::time::Instant::now() < deadline);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(e) => panic!("unexpected expiry error: {e}"),
        }
    }
    assert!(matches!(leader.renew().await, Err(MemberError::NotLeader)));
    let next = member.campaign(5).await.unwrap();
    assert!(next.revision() > leader.revision());
    assert!(matches!(
        leader
            .transact(vec![], vec![Mutation::put(b"/expired", b"x")])
            .await,
        Err(MemberError::NotLeader)
    ));
    eprintln!(
        "natural lease expiry: lease={} fenced, replacement revision={}",
        leader.lease_id(),
        next.revision()
    );
    next.resign().await.unwrap();
}
#[tokio::test]
async fn memory_natural_expiry() {
    expiry(Arc::new(MemoryStore::default())).await;
}
