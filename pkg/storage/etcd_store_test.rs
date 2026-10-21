// Copyright 2026 AsterSQL.
use astersql_pd_storage::*;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
#[path = "contract_test.rs"]
mod contract;
#[path = "etcd_fixture_test.rs"]
mod fixture;
use fixture::Fixture;

macro_rules! contract_case {
    ($($name:ident),* $(,)?) => {$ (
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() { let fixture = Fixture::start(); contract::$name(&fixture.store().await).await; }
    )*};
}
contract_case!(
    cas_atomic_revision,
    snapshot_scan,
    watch_replay,
    compaction_recovery,
    lease_expiry,
    lease_renew_revoke,
    election_fencing,
    invalid_atomic,
    watch_cancel
);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnect_restart_resume_and_cancel() {
    let mut fixture = Fixture::start();
    let store = fixture.store().await;
    let first = contract::put(&store, b"/fault/a", b"one").await;
    let mut watch = store
        .watch(b"/fault/".to_vec(), first.revision)
        .await
        .unwrap();
    assert_eq!(
        contract::batch(watch.as_mut()).await.revision,
        first.revision
    );
    let lease = store.grant(2).await.unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: b"/fault/leased".to_vec(),
                value: vec![],
                lease: Some(lease.id),
            }],
        )
        .await
        .unwrap();
    let leased_batch = contract::batch(watch.as_mut()).await;
    fixture.command("stop", "stopped");
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        watch.next(&CancellationToken::new()),
    )
    .await
    .expect("stream failed to notice disconnect")
    .unwrap_err();
    assert_eq!(error, StoreError::Unavailable);
    assert_eq!(
        store
            .read(Read::prefix(b"/fault/", Consistency::Linearizable))
            .await
            .unwrap_err(),
        StoreError::Unavailable
    );
    // Cancellation also interrupts reconnect when the endpoint is down.
    let cancel = CancellationToken::new();
    let mut cancelled = store
        .watch(b"/fault/".to_vec(), leased_batch.revision + 1)
        .await
        .unwrap();
    cancel.cancel();
    assert_eq!(
        cancelled.next(&cancel).await.unwrap_err(),
        StoreError::Cancelled
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    fixture.command("start", "restarted");
    let recovered = fixture.store().await;
    let commit = contract::put(&recovered, b"/fault/after", b"two").await;
    let mut observed = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        match tokio::time::timeout_at(deadline, watch.next(&CancellationToken::new()))
            .await
            .unwrap()
        {
            Ok(batch) => {
                assert!(
                    batch
                        .events
                        .iter()
                        .all(|e| e.revision() > leased_batch.revision)
                );
                let reached = batch.revision >= commit.revision;
                observed.extend(batch.events);
                if reached {
                    break;
                }
            }
            Err(StoreError::Unavailable) => tokio::time::sleep(Duration::from_millis(100)).await,
            Err(e) => panic!("unexpected resume error: {e}"),
        }
    }
    assert!(observed.iter().any(|e| e.key() == b"/fault/after"));
    // Restart does not guarantee immediate lease expiry; wait for committed invalidation.
    let end = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match recovered.ttl(lease.id).await {
            Err(StoreError::LeaseNotFound) => break,
            Ok(_) => {
                assert!(tokio::time::Instant::now() < end);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => panic!("lease recovery: {e}"),
        }
    }
    assert!(
        contract::read(&recovered, b"/fault/leased")
            .await
            .entries
            .is_empty()
    );
    assert_eq!(
        recovered.keep_alive(lease.id).await.unwrap_err(),
        StoreError::LeaseNotFound
    );
    eprintln!(
        "fault recovery: disconnected, resumed after {}, persisted commit {}, lease {} expired",
        leased_batch.revision, commit.revision, lease.id
    );
}
