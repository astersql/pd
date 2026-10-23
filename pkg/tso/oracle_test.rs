// Copyright 2026 AsterSQL.
use astersql_pd_member::{Member, MemberError};
use astersql_pd_storage_kv::*;
use astersql_pd_tso::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio_util::sync::CancellationToken;

pub struct ManualClock(pub AtomicU64);
impl Clock for ManualClock {
    fn now_millis(&self) -> std::result::Result<u64, TsoError> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
pub async fn boundary(store: &Arc<dyn MetaStore>, cluster: u64) -> u64 {
    let entries = store
        .read(Read::prefix(
            format!("/pd/{cluster}/timestamp").as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    u64::from_be_bytes(entries[0].value.clone().try_into().unwrap())
}
pub async fn behavior(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 71, 1, b"one".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let clock = Arc::new(ManualClock(AtomicU64::new(1000)));
    let oracle = Arc::new(
        Oracle::new(
            store.clone(),
            leader.clone(),
            clock.clone(),
            cancel.clone(),
            10,
        )
        .await
        .unwrap(),
    );
    let mut workers = Vec::new();
    for _ in 0..16 {
        let oracle = oracle.clone();
        workers.push(tokio::spawn(async move {
            let mut values = Vec::new();
            for _ in 0..20 {
                values.push(oracle.allocate(1).await.unwrap());
            }
            assert!(values.windows(2).all(|w| w[0] < w[1]));
            values
        }));
    }
    let mut values = Vec::new();
    for worker in workers {
        values.extend(worker.await.unwrap());
    }
    values.sort();
    assert_eq!(values.len(), 320);
    assert!(values.windows(2).all(|w| w[0] < w[1]));
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("compat_test.json")).unwrap();
    let check = |name: &str, timestamp: Timestamp| {
        assert_eq!(
            serde_json::json!([timestamp.physical, timestamp.logical]),
            vectors[name]
        );
    };
    let first = values[0];
    let last = *values.last().unwrap();
    assert_eq!(
        first,
        Timestamp {
            physical: 1000,
            logical: 1
        }
    );
    assert_eq!(last.logical, 320);
    check("concurrent", last);
    eprintln!(
        "concurrent first={first:?} last={last:?} persisted-ns={}",
        boundary(&store, 71).await
    );
    clock.0.store(500, Ordering::SeqCst);
    oracle.update().await.unwrap();
    let rollback = oracle.allocate(1).await.unwrap();
    check("rollback", rollback);
    assert!(rollback > last);
    assert_eq!(rollback.physical, last.physical);
    eprintln!(
        "rollback first={last:?} last={rollback:?} persisted-ns={}",
        boundary(&store, 71).await
    );
    let high = oracle
        .allocate(MAX_LOGICAL - rollback.logical - 1)
        .await
        .unwrap();
    check("before_overflow", high);
    let overflow = oracle.allocate(2).await.unwrap();
    check("overflow", overflow);
    assert_eq!(overflow.physical, high.physical + 1);
    assert_eq!(overflow.logical, 2);
    eprintln!(
        "overflow first={high:?} last={overflow:?} persisted-ns={}",
        boundary(&store, 71).await
    );
    clock.0.store(2000, Ordering::SeqCst);
    oracle.update().await.unwrap();
    let forward = oracle.allocate(1).await.unwrap();
    assert_eq!(forward.physical, 2000);
    assert!(boundary(&store, 71).await > forward.physical * 1_000_000);
    store.revoke(leader.lease_id()).await.unwrap();
    assert!(matches!(
        oracle.allocate(1).await,
        Err(TsoError::Member(MemberError::NotLeader))
    ));
    assert!(oracle.update().await.is_err());
    let leader2 = Arc::new(member.campaign(30).await.unwrap());
    let oracle2 = Oracle::new(store.clone(), leader2.clone(), clock, cancel.clone(), 10)
        .await
        .unwrap();
    let next = oracle2.allocate(1).await.unwrap();
    assert!(next > forward);
    assert!(matches!(
        oracle.allocate(1).await,
        Err(TsoError::Member(MemberError::NotLeader))
    ));
    eprintln!(
        "switch first={forward:?} last={next:?} persisted-ns={}",
        boundary(&store, 71).await
    );
    assert!(matches!(
        oracle2.allocate(0).await,
        Err(TsoError::InvalidArgument)
    ));
    assert!(matches!(
        oracle2.allocate(MAX_LOGICAL).await,
        Err(TsoError::InvalidArgument)
    ));
    cancel.cancel();
    assert!(matches!(
        oracle2.allocate(1).await,
        Err(TsoError::Cancelled)
    ));
    leader2.resign().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn memory_behavior() {
    behavior(Arc::new(MemoryStore::default())).await;
}

pub async fn boundaries(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 73, 1, b"bounds".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let clock = Arc::new(ManualClock(AtomicU64::new(1000)));
    let oracle = Oracle::new(
        store.clone(),
        leader.clone(),
        clock.clone(),
        cancel.clone(),
        2,
    )
    .await
    .unwrap();
    let first = oracle.allocate(MAX_LOGICAL - 1).await.unwrap();
    let saved = boundary(&store, 73).await;
    assert!(matches!(
        oracle.allocate(1).await,
        Err(TsoError::WindowExhausted)
    ));
    assert_eq!(boundary(&store, 73).await, saved);
    oracle.update().await.unwrap();
    let last = oracle.allocate(1).await.unwrap();
    assert_eq!(
        last,
        Timestamp {
            physical: 1001,
            logical: 1
        }
    );
    assert!(boundary(&store, 73).await > saved);
    eprintln!(
        "window exhausted/update first={first:?} last={last:?} previous-ns={saved} persisted-ns={}",
        boundary(&store, 73).await
    );
    let batch_oracle = Oracle::new(
        store.clone(),
        leader.clone(),
        clock.clone(),
        cancel.clone(),
        2,
    )
    .await
    .unwrap();
    let batch_first = batch_oracle.allocate(1).await.unwrap();
    assert!(matches!(
        batch_oracle.allocate(MAX_LOGICAL - 1).await,
        Err(TsoError::WindowExhausted)
    ));
    batch_oracle.update().await.unwrap();
    let batch_last = batch_oracle.allocate(MAX_LOGICAL - 1).await.unwrap();
    assert!(batch_last.physical > batch_first.physical);
    // Go stores an eight-byte big-endian UnixNano, including sub-ms values.
    let nanos = 4_000_000_001u64;
    store
        .transact(
            vec![],
            vec![Mutation::put(b"/pd/73/timestamp", &nanos.to_be_bytes())],
        )
        .await
        .unwrap();
    let recovered = Oracle::new(
        store.clone(),
        leader.clone(),
        clock.clone(),
        cancel.clone(),
        2,
    )
    .await
    .unwrap();
    let result = recovered.allocate(1).await.unwrap();
    assert_eq!(result.physical, 4002);
    assert!(result.physical * 1_000_000 >= nanos + 1_000_000);
    for invalid in [vec![1, 2, 3], u64::MAX.to_be_bytes().to_vec()] {
        store
            .transact(vec![], vec![Mutation::put(b"/pd/73/timestamp", &invalid)])
            .await
            .unwrap();
        assert!(matches!(
            Oracle::new(
                store.clone(),
                leader.clone(),
                clock.clone(),
                cancel.clone(),
                2
            )
            .await,
            Err(TsoError::CorruptBoundary)
        ));
    }
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/pd/73/timestamp".to_vec(),
            }],
        )
        .await
        .unwrap();
    clock.0.store(u64::MAX, Ordering::SeqCst);
    assert!(matches!(
        Oracle::new(
            store.clone(),
            leader.clone(),
            clock.clone(),
            cancel.clone(),
            2
        )
        .await,
        Err(TsoError::Exhausted)
    ));
    assert!(matches!(oracle.update().await, Err(TsoError::Exhausted)));
    leader.resign().await.unwrap();
}
#[tokio::test]
async fn memory_boundaries() {
    boundaries(Arc::new(MemoryStore::default())).await;
}
