// Copyright 2026 AsterSQL.
use super::store_transition;
use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta, StoreState};
use std::{collections::BTreeSet, sync::Arc};

#[test]
fn task_052_no_leader_destination() {
    let cluster = BasicCluster::default();
    for id in 1..=4 {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            node_state: NodeState::Serving,
            ..Default::default()
        })));
    }
    let evicted = BTreeSet::from([2, 3, 4]);
    assert_eq!(
        store_transition(&cluster, 1, Some(false), 3, &evicted)
            .err()
            .unwrap(),
        "[PD:core:ErrNoStoreForRegionLeader]can not remove store 1 since there are no extra up store to store the leader"
    );
    assert!(cluster.stores.get_store(1).unwrap().is_up());
    let next = store_transition(&cluster, 1, Some(true), 3, &evicted)
        .unwrap()
        .unwrap();
    assert_eq!(next.state(), StoreState::Offline);
    assert!(next.is_physically_destroyed());
    assert!(
        cluster.stores.get_store(1).unwrap().is_up(),
        "validation must not publish before persistence"
    );
    assert!(
        store_transition(&cluster, 1, Some(false), 3, &BTreeSet::from([2, 3]))
            .unwrap()
            .is_some()
    );
}
