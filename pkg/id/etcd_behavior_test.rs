// Copyright 2026 AsterSQL.
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;
#[path = "allocator_test.rs"]
mod scenario;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_etcd_behavior() {
    let fixture = fixture::Fixture::start();
    scenario::behavior(std::sync::Arc::new(fixture.store().await)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_etcd_boundaries() {
    let fixture = fixture::Fixture::start();
    scenario::boundaries(std::sync::Arc::new(fixture.store().await)).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_etcd_wal_restart() {
    use astersql_pd_id::Allocator;
    use astersql_pd_member::Member;
    use astersql_pd_storage_kv::*;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let member = Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        45,
        1,
        b"persistent".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(5).await.unwrap());
    let allocator = Allocator::new(
        store.clone(),
        leader.clone(),
        CancellationToken::new(),
        1000,
    )
    .unwrap();
    assert_eq!(allocator.alloc().await.unwrap(), 1);
    leader.resign().await.unwrap();
    drop(allocator);
    drop(leader);
    drop(member);
    drop(store);
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let member = Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        45,
        1,
        b"persistent".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(5).await.unwrap());
    let allocator = Allocator::new(store, leader.clone(), CancellationToken::new(), 1000).unwrap();
    assert_eq!(allocator.alloc().await.unwrap(), 1001);
    eprintln!("etcd WAL restart: previous ID=1 persisted boundary=1000 next=1001");
    leader.resign().await.unwrap();
}
