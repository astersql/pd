// Copyright 2026 AsterSQL.
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;
#[path = "member_test.rs"]
mod scenario;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_etcd_behavior() {
    let fixture = fixture::Fixture::start();
    scenario::behavior(std::sync::Arc::new(fixture.store().await)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_etcd_natural_expiry() {
    let fixture = fixture::Fixture::start();
    scenario::expiry(std::sync::Arc::new(fixture.store().await)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_etcd_identity_recovery() {
    use astersql_pd_member::{Member, MemberError};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    let mut fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let member = Member::bootstrap(store, CancellationToken::new(), 46, 7, b"durable".to_vec())
        .await
        .unwrap();
    let leader = member.campaign(5).await.unwrap();
    leader.resign().await.unwrap();
    drop(leader);
    drop(member);
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let store = Arc::new(fixture.store().await);
    assert!(matches!(
        Member::bootstrap(
            store.clone(),
            CancellationToken::new(),
            46,
            7,
            b"changed".to_vec()
        )
        .await,
        Err(MemberError::IdentityConflict)
    ));
    let member = Member::bootstrap(store, CancellationToken::new(), 46, 7, b"durable".to_vec())
        .await
        .unwrap();
    member.campaign(5).await.unwrap().resign().await.unwrap();
}
