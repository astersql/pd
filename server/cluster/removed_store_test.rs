// Copyright 2026 AsterSQL.
use super::{handle_store_heartbeat, removed_store::handle_checked};
use astersql_pd_core::{
    BasicCluster, KeyRange, NodeState, Peer, PeerStat, RegionInfo, RegionMeta, StoreInfo,
    StoreMeta, StoreState, StoreStats, TimeInterval,
};
use astersql_pd_statistics::HotStat;
use astersql_pd_statistics_utils::RWType;
use prometheus::Registry;
use std::sync::{Arc, Barrier};
use tokio::runtime::Handle;

fn cluster() -> BasicCluster {
    let mut c = BasicCluster::new();
    c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    let peer = Peer {
        id: 11,
        store_id: 1,
        ..Default::default()
    };
    c.regions.put_region(Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: vec![peer.clone()],
            key_range: KeyRange {
                start_key: vec![1],
                end_key: vec![2],
            },
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    )));
    c
}
fn report() -> StoreStats {
    StoreStats {
        store_id: 1,
        capacity: 999,
        interval: Some(TimeInterval {
            start_timestamp: 0,
            end_timestamp: 10,
        }),
        peer_stats: vec![PeerStat {
            region_id: 1,
            read_bytes: 10_000_000,
            read_keys: 10_000_000,
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn remove(c: &BasicCluster) {
    assert!(c.stores.patch_store(1, |s| s.edit_meta(|m| {
        m.state = StoreState::Tombstone;
        m.node_state = NodeState::Removed;
    })));
}

#[tokio::test]
async fn state_task_42_removed_store_routine_noop() {
    let mut c = cluster();
    let mut h = HotStat::new(&Handle::current(), &Registry::new(), 128).unwrap();
    let seed = StoreStats {
        store_id: 1,
        capacity: 100,
        ..Default::default()
    };
    handle_store_heartbeat(&mut c, Some(&h), &seed, 100).unwrap();
    remove(&c);
    let before = c.stores.get_store(1).unwrap();
    for _ in 0..3 {
        assert_eq!(
            handle_store_heartbeat(&mut c, Some(&h), &report(), 200).unwrap(),
            NodeState::Removed
        );
    }
    let after = c.stores.get_store(1).unwrap();
    assert!(Arc::ptr_eq(&before, &after));
    assert_eq!(after.store_stats(), seed);
    assert_eq!(after.last_heartbeat_unix_nanos(), 100);
    assert!(h.stores_stats.get_rolling_store_stats(1).is_none());
    assert!(
        h.hot_cache
            .get_hot_peer_stats(RWType::READ, 0)
            .await
            .unwrap()
            .is_empty()
    );
    h.shutdown().await.unwrap();
}

async fn concurrent_removal(delete: bool) {
    let c = cluster();
    let mut h = HotStat::new(&Handle::current(), &Registry::new(), 128).unwrap();
    let before = c.stores.get_store(1).unwrap();
    let raw = before.store_stats();
    let checked = Barrier::new(2);
    let removed = Barrier::new(2);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            checked.wait();
            remove(&c);
            if delete {
                c.stores.delete_store(1);
            }
            removed.wait();
        });
        // Exact check -> remove -> commit order; no sleeps or timing guesses.
        handle_checked(&c, Some(&h), &report(), 200, || {
            checked.wait();
            removed.wait();
        })
        .unwrap();
        worker.join().unwrap();
    });
    assert_eq!(
        before.store_stats(),
        raw,
        "late report changed shared raw holder"
    );
    if delete {
        assert!(
            c.stores.get_store(1).is_none(),
            "late heartbeat resurrected membership"
        );
    } else {
        let store = c.stores.get_store(1).unwrap();
        assert!(store.is_removed());
        assert_eq!(store.last_heartbeat_unix_nanos(), 0);
    }
    assert!(h.stores_stats.get_rolling_store_stats(1).is_none());
    assert!(
        h.hot_cache
            .get_hot_peer_stats(RWType::READ, 0)
            .await
            .unwrap()
            .is_empty()
    );
    h.shutdown().await.unwrap();
}
#[tokio::test]
async fn state_task_42_removed_store_between_check_and_commit() {
    concurrent_removal(false).await;
}
#[tokio::test]
async fn state_task_42_removed_store_deleted_between_check_and_commit() {
    concurrent_removal(true).await;
}

#[test]
fn state_task_42_removed_store_commit_preserves_current_metadata() {
    let c = cluster();
    handle_checked(&c, None, &report(), 200, || {
        c.stores
            .patch_store(1, |s| s.edit_meta(|m| m.node_state = NodeState::Removing));
    })
    .unwrap();
    let store = c.stores.get_store(1).unwrap();
    assert_eq!(store.node_state(), NodeState::Removing);
    assert_eq!(store.store_stats(), report());
    assert_eq!(store.last_heartbeat_unix_nanos(), 200);
}
