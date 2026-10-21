// Copyright 2026 AsterSQL.
use crate::oracle_test::{ManualClock, boundary};
use astersql_pd_member::Member;
use astersql_pd_storage::*;
use astersql_pd_tso::*;
use std::sync::{Arc, atomic::AtomicU64};
use tokio_util::sync::CancellationToken;

pub async fn restart(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 72, 1, b"one".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let clock = Arc::new(ManualClock(AtomicU64::new(1000)));
    let old = Oracle::new(
        store.clone(),
        leader.clone(),
        clock.clone(),
        cancel.clone(),
        10,
    )
    .await
    .unwrap();
    let first = old.allocate(1).await.unwrap();
    let saved = boundary(&store, 72).await;
    let new = Oracle::new(
        store.clone(),
        leader.clone(),
        clock.clone(),
        cancel.clone(),
        10,
    )
    .await
    .unwrap();
    let last = new.allocate(1).await.unwrap();
    assert!(last.physical * 1_000_000 > saved);
    // A second oracle cannot keep issuing from an earlier window in the same term.
    assert!(matches!(old.allocate(1).await, Err(TsoError::WindowLost)));
    eprintln!(
        "restart first={first:?} last={last:?} previous-ns={saved} persisted-ns={}",
        boundary(&store, 72).await
    );
    leader.resign().await.unwrap();
}
#[tokio::test]
async fn memory_restart() {
    restart(Arc::new(MemoryStore::default())).await;
}
