// Copyright 2026 AsterSQL.
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;
#[path = "oracle_test.rs"]
mod oracle_test;
#[path = "restart_test.rs"]
mod restart_test;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_etcd_behavior() {
    let f = fixture::Fixture::start();
    oracle_test::behavior(std::sync::Arc::new(f.store().await)).await;
}
#[tokio::test]
async fn real_etcd_restart() {
    let f = fixture::Fixture::start();
    restart_test::restart(std::sync::Arc::new(f.store().await)).await;
}

#[tokio::test]
async fn real_etcd_boundaries() {
    let f = fixture::Fixture::start();
    oracle_test::boundaries(std::sync::Arc::new(f.store().await)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_etcd_wal_restart_and_outage() {
    use astersql_pd_member::Member;
    use astersql_pd_storage_kv::*;
    use astersql_pd_tso::*;
    use std::sync::{Arc, atomic::AtomicU64};
    use tokio_util::sync::CancellationToken;
    let mut f = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 74, 1, b"durable".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let clock = Arc::new(oracle_test::ManualClock(AtomicU64::new(1000)));
    let oracle = Oracle::new(
        store.clone(),
        leader.clone(),
        clock.clone(),
        cancel.clone(),
        10,
    )
    .await
    .unwrap();
    let first = oracle.allocate(1).await.unwrap();
    let saved = oracle_test::boundary(&store, 74).await;
    f.command("stop", "stopped");
    assert!(
        oracle.allocate(1).await.is_err(),
        "cached window must fail closed during outage"
    );
    drop(oracle);
    drop(leader);
    drop(member);
    drop(store);
    f.command("start", "restarted");
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let member = Member::bootstrap(store.clone(), cancel.clone(), 74, 1, b"durable".to_vec())
        .await
        .unwrap();
    // The old lease survived restart: revoke it via the persisted leader entry.
    let lease = store
        .read(Read::prefix(b"/pd/74/leader", Consistency::Linearizable))
        .await
        .unwrap()
        .entries[0]
        .lease
        .unwrap();
    store.revoke(lease).await.unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let oracle = Oracle::new(store.clone(), leader.clone(), clock, cancel, 10)
        .await
        .unwrap();
    let last = oracle.allocate(1).await.unwrap();
    assert!(last.physical * 1_000_000 > saved);
    eprintln!(
        "etcd WAL/outage first={first:?} last={last:?} previous-ns={saved} persisted-ns={}",
        oracle_test::boundary(&store, 74).await
    );
    leader.resign().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_etcd_natural_lease_expiry() {
    use astersql_pd_member::{Member, MemberError};
    use astersql_pd_storage_kv::MetaStore;
    use astersql_pd_tso::*;
    use std::sync::{Arc, atomic::AtomicU64};
    use tokio_util::sync::CancellationToken;
    let f = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let member = Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        75,
        1,
        b"expiry".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(1).await.unwrap());
    let oracle = Oracle::new(
        store.clone(),
        leader.clone(),
        Arc::new(oracle_test::ManualClock(AtomicU64::new(1000))),
        CancellationToken::new(),
        10,
    )
    .await
    .unwrap();
    oracle.allocate(1).await.unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
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
    assert!(matches!(
        oracle.allocate(1).await,
        Err(TsoError::Member(MemberError::NotLeader))
    ));
}

// Run this very test in a child process, then terminate without destructors to
// prove that no in-process state or graceful oracle cleanup is needed to recover.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_etcd_oracle_process_restart() {
    use astersql_pd_member::Member;
    use astersql_pd_storage_kv::*;
    use astersql_pd_tso::*;
    use std::{
        io::Write,
        sync::{Arc, atomic::AtomicU64},
        time::Duration,
    };
    use tokio_util::sync::CancellationToken;
    if let Ok(endpoint) = std::env::var("PD_TSO_PROCESS_TEST_ENDPOINT") {
        let store: Arc<dyn MetaStore> = Arc::new(
            EtcdMetaStore::connect(&[endpoint], Duration::from_secs(2))
                .await
                .unwrap(),
        );
        let cancel = CancellationToken::new();
        let member = Member::bootstrap(store.clone(), cancel.clone(), 76, 1, b"process".to_vec())
            .await
            .unwrap();
        let leader = Arc::new(member.campaign(30).await.unwrap());
        let oracle = Oracle::new(
            store.clone(),
            leader,
            Arc::new(oracle_test::ManualClock(AtomicU64::new(1000))),
            cancel,
            10,
        )
        .await
        .unwrap();
        let timestamp = oracle.allocate(1).await.unwrap();
        let saved = oracle_test::boundary(&store, 76).await;
        // Serial libtest prefixes the test name without ending its output line.
        println!(
            "\nPROCESS_TSO {} {} {}",
            timestamp.physical, timestamp.logical, saved
        );
        std::io::stdout().flush().unwrap();
        std::process::exit(0); // test-only abrupt termination; no Drop/lease resignation
    }
    let f = fixture::Fixture::start();
    let mut observations = Vec::new();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    for _ in 0..2 {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "real_etcd_oracle_process_restart", "--nocapture"])
            .env("PD_TSO_PROCESS_TEST_ENDPOINT", &f.endpoint)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let fields: Vec<u64> = stdout
            .lines()
            .find_map(|line| line.strip_prefix("PROCESS_TSO "))
            .unwrap_or_else(|| panic!("child allocated a timestamp; stdout={stdout:?}"))
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        observations.push(fields);
        let lease = store
            .read(Read::prefix(b"/pd/76/leader", Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .lease
            .unwrap();
        // Deterministic failover after the process exited, retaining its durable window.
        store.revoke(lease).await.unwrap();
    }
    assert!(observations[1][0] * 1_000_000 > observations[0][2]);
    assert_eq!(observations[0], vec![1000, 1, 1_010_000_000]);
    assert_eq!(observations[1], vec![1011, 1, 1_021_000_000]);
    eprintln!(
        "oracle abrupt process restart first={:?} last={:?}",
        observations[0], observations[1]
    );
}
