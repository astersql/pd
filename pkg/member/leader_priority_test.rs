// Copyright 2026 AsterSQL.
use super::{LeaderPriority, LeaderPriorityError, Leadership};
use astersql_pd_storage_kv::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Reuse one fixture counter per test process to keep data directories isolated.
use super::member_info_test::fixture;

async fn behavior(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let priority = LeaderPriority::new(store.clone(), cancel.clone(), 42);
    assert_eq!(priority.get_member_leader_priority(7).await.unwrap(), 0);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        42,
        b"leader".to_vec(),
        30,
    )
    .await
    .unwrap();
    priority
        .delete_member_leader_priority(&term, 7)
        .await
        .unwrap();
    // Go strconv itob64tests: TestItoa verifies these exact decimal bytes.
    for (input, bytes) in [
        (0, "0"),
        (1, "1"),
        (-1, "-1"),
        (2147483647, "2147483647"),
        (-2147483648, "-2147483648"),
    ] {
        priority
            .set_member_leader_priority(&term, 7, input)
            .await
            .unwrap();
        assert_eq!(
            priority.get_member_leader_priority(7).await.unwrap(),
            input as i32
        );
        let rows = store
            .read(Read::range(
                b"/pd/42/member/7/leader_priority",
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap()
            .entries;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, bytes.as_bytes());
        assert_eq!(rows[0].lease, None);
    }
    // Go accepts the full int on write, but ParseInt(..., 10, 32) rejects overflow on read.
    for (input, bytes) in [
        (2147483648, "2147483648"),
        (-2147483649, "-2147483649"),
        (i64::MAX, "9223372036854775807"),
        (i64::MIN, "-9223372036854775808"),
    ] {
        priority
            .set_member_leader_priority(&term, 7, input)
            .await
            .unwrap();
        assert!(matches!(
            priority.get_member_leader_priority(7).await,
            Err(LeaderPriorityError::InvalidPriority)
        ));
        assert_eq!(
            store
                .read(Read::range(
                    b"/pd/42/member/7/leader_priority",
                    b"",
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries[0]
                .value,
            bytes.as_bytes()
        );
    }
    for (bytes, expected) in [
        ("+12", 12),
        ("-0", 0),
        ("012345", 12345),
        ("-012345", -12345),
    ] {
        store
            .transact(
                vec![],
                vec![Mutation::put(
                    b"/pd/42/member/7/leader_priority",
                    bytes.as_bytes(),
                )],
            )
            .await
            .unwrap();
        assert_eq!(
            priority.get_member_leader_priority(7).await.unwrap(),
            expected
        );
    }
    for bytes in [
        b"".as_slice(),
        b"1_2",
        b" 1",
        b"1\n",
        b"0x10",
        b"+",
        b"1.0",
        b"12345x",
        &[0xff],
        b"1\0",
    ] {
        store
            .transact(
                vec![],
                vec![Mutation::put(b"/pd/42/member/7/leader_priority", bytes)],
            )
            .await
            .unwrap();
        assert!(
            matches!(
                priority.get_member_leader_priority(7).await,
                Err(LeaderPriorityError::InvalidPriority)
            ),
            "{bytes:?}"
        );
    }
    priority
        .set_member_leader_priority(&term, 7, -1)
        .await
        .unwrap();
    priority
        .set_member_leader_priority(&term, 8, 12)
        .await
        .unwrap();
    let other = LeaderPriority::new(store.clone(), CancellationToken::new(), 43);
    let other_term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        43,
        b"other".to_vec(),
        30,
    )
    .await
    .unwrap();
    other
        .set_member_leader_priority(&other_term, 7, 99)
        .await
        .unwrap();
    assert!(matches!(
        other.set_member_leader_priority(&term, 7, 0).await,
        Err(LeaderPriorityError::InvalidTerm)
    ));
    let foreign = LeaderPriority::new(
        Arc::new(MemoryStore::default()),
        CancellationToken::new(),
        42,
    );
    assert!(matches!(
        foreign.delete_member_leader_priority(&term, 7).await,
        Err(LeaderPriorityError::InvalidTerm)
    ));
    priority
        .delete_member_leader_priority(&term, 7)
        .await
        .unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/42/member/7/leader_priority/extra",
                b"123",
            )],
        )
        .await
        .unwrap();
    assert_eq!(priority.get_member_leader_priority(7).await.unwrap(), 0);
    assert_eq!(priority.get_member_leader_priority(8).await.unwrap(), 12);
    assert_eq!(other.get_member_leader_priority(7).await.unwrap(), 99);
    // Lose the old term, reelect the same value, and attempt stale writes/deletes.
    term.resign().await.unwrap();
    let successor = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        42,
        b"leader".to_vec(),
        30,
    )
    .await
    .unwrap();
    for result in [
        priority.set_member_leader_priority(&term, 8, 100).await,
        priority.delete_member_leader_priority(&term, 8).await,
        priority.delete_member_leader_priority(&term, 7).await,
    ] {
        assert!(
            matches!(result, Err(LeaderPriorityError::Conflict)),
            "{result:?}"
        );
    }
    assert_eq!(priority.get_member_leader_priority(8).await.unwrap(), 12);
    priority
        .set_member_leader_priority(&successor, 8, -1)
        .await
        .unwrap();
    cancel.cancel();
    assert!(matches!(
        priority.get_member_leader_priority(8).await,
        Err(LeaderPriorityError::Cancelled)
    ));
    for result in [
        priority
            .set_member_leader_priority(&successor, 8, 100)
            .await,
        priority.delete_member_leader_priority(&successor, 8).await,
    ] {
        assert!(matches!(result, Err(LeaderPriorityError::Cancelled)));
    }
    let fresh = LeaderPriority::new(store, CancellationToken::new(), 42);
    successor.resign().await.unwrap();
    assert_eq!(fresh.get_member_leader_priority(8).await.unwrap(), -1);
    other_term.resign().await.unwrap();
}
#[tokio::test]
async fn leader_priority_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn leader_priority_etcd() {
    let fixture = fixture::Fixture::start();
    behavior(Arc::new(fixture.store().await)).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn leader_priority_etcd_io_failure() {
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let priority = LeaderPriority::new(store.clone(), CancellationToken::new(), 44);
    let term = Leadership::campaign(store, CancellationToken::new(), 44, b"leader".to_vec(), 30)
        .await
        .unwrap();
    priority
        .set_member_leader_priority(&term, 7, -1)
        .await
        .unwrap();
    fixture.command("stop", "stopped");
    eprintln!("leader_priority: etcd stopped after acknowledged -1; read/write/delete must fail");
    assert!(matches!(
        priority.get_member_leader_priority(7).await,
        Err(LeaderPriorityError::Store(StoreError::Unavailable))
    ));
    for result in [
        priority.set_member_leader_priority(&term, 7, 100).await,
        priority.delete_member_leader_priority(&term, 7).await,
    ] {
        assert!(
            matches!(
                result,
                Err(LeaderPriorityError::Store(StoreError::Unavailable))
            ),
            "{result:?}"
        );
    }
    fixture.command("start", "restarted");
    let fresh = LeaderPriority::new(
        Arc::new(fixture.store().await),
        CancellationToken::new(),
        44,
    );
    assert_eq!(fresh.get_member_leader_priority(7).await.unwrap(), -1);
    term.resign().await.unwrap();
    eprintln!("leader_priority: same WAL restart retains -1 after failed writes/deletes");
}
