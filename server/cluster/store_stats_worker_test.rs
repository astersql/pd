// Copyright 2026 AsterSQL.
use super::StoreStatsWorker;
use astersql_pd_core::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test(start_paused = true)]
async fn task_013_behavior() {
    let cluster = Arc::new(Mutex::new(BasicCluster::new()));
    cluster
        .lock()
        .unwrap()
        .stores
        .put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: 1,
            node_state: NodeState::Serving,
            ..Default::default()
        })));
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    ));
    {
        let mut c = cluster.lock().unwrap();
        let (old, overlaps, changed) = c.regions.set_region(region.clone());
        c.regions
            .update_sub_tree(region, old.as_deref(), &overlaps, changed);
    }
    let cancel = CancellationToken::new();
    let c = cluster.clone();
    let worker = StoreStatsWorker::start(&tokio::runtime::Handle::current(), &cancel, move || {
        let c = c.clone();
        async move {
            super::refresh_store_stats(&c.lock().unwrap());
        }
    });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(8)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        cluster
            .lock()
            .unwrap()
            .stores
            .get_store(1)
            .unwrap()
            .region_count(),
        0
    );
    tokio::time::advance(Duration::from_millis(2)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        cluster
            .lock()
            .unwrap()
            .stores
            .get_store(1)
            .unwrap()
            .region_count(),
        1,
        "tick must publish region counters"
    );
    tokio::time::advance(Duration::from_millis(20)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        cluster
            .lock()
            .unwrap()
            .stores
            .get_store(1)
            .unwrap()
            .region_count(),
        1
    );
    worker.stop().await.unwrap();
    cluster.lock().unwrap().regions.remove_region_if_exist(1);
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        cluster
            .lock()
            .unwrap()
            .stores
            .get_store(1)
            .unwrap()
            .region_count(),
        1
    );
    assert_eq!(Arc::strong_count(&cluster), 1);
}

#[test]
fn task_013_removed_and_empty_stores() {
    let cluster = BasicCluster::new();
    for (id, state) in [(1, NodeState::Serving), (2, NodeState::Removed)] {
        let mut store = StoreInfo::new(StoreMeta {
            id,
            node_state: state,
            ..Default::default()
        });
        store.set_region_count(99);
        store.set_leader_size(99);
        cluster.stores.put_store(Arc::new(store));
    }
    let previous = cluster.stores.get_store(1).unwrap();
    super::refresh_store_stats(&cluster);
    super::refresh_store_stats(&cluster);
    assert_eq!(cluster.stores.get_store(1).unwrap().region_count(), 0);
    assert_eq!(cluster.stores.get_store(1).unwrap().leader_size(), 0);
    assert_eq!(cluster.stores.get_store(2).unwrap().region_count(), 99);
    assert_eq!(previous.region_count(), 99);
}
