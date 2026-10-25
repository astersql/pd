// Copyright 2026 AsterSQL.
//! Source-compatible test support. Never linked into production scheduling.
use crate::{OpStep, Operator};
use astersql_pd_core::{BasicCluster, Peer, PeerRole, RegionInfo, RegionOption, StoreStats};
use std::sync::Arc;

/// Apply the first unfinished basic step to a real immutable region snapshot.
/// Unsupported unfinished steps and inconsistent membership panic like Go.
/// Completion is acknowledged by the next call, through canonical Operator::check.
pub(crate) fn apply_operator_step(region: &RegionInfo, op: &Operator) -> RegionInfo {
    let _ = op.start();
    let Some(step) = op.check(region) else {
        return region.clone();
    };
    match step {
        OpStep::TransferLeader(s) => region.clone_with([RegionOption::Leader(
            region.get_store_peer(s.to_store).cloned(),
        )]),
        OpStep::AddPeer(s) => {
            assert!(
                region.get_store_peer(s.to_store).is_none(),
                "Add peer that exists"
            );
            region.clone_with([RegionOption::AddPeer(Peer {
                id: s.peer_id,
                store_id: s.to_store,
                ..Default::default()
            })])
        }
        OpStep::AddLearner(s) => {
            assert!(
                region.get_store_peer(s.to_store).is_none(),
                "Add learner that exists"
            );
            region.clone_with([RegionOption::AddPeer(Peer {
                id: s.peer_id,
                store_id: s.to_store,
                role: PeerRole::Learner,
                is_witness: false,
            })])
        }
        OpStep::PromoteLearner(s) => {
            assert!(
                region.get_store_learner(s.to_store).is_some(),
                "Promote peer that doesn't exist"
            );
            region.clone_with([
                RegionOption::RemoveStorePeer(s.to_store),
                RegionOption::AddPeer(Peer {
                    id: s.peer_id,
                    store_id: s.to_store,
                    ..Default::default()
                }),
            ])
        }
        OpStep::RemovePeer(s) => {
            assert!(
                region.get_store_peer(s.from_store).is_some(),
                "Remove peer that doesn't exist"
            );
            assert!(
                region.leader().map_or(0, |p| p.store_id) != s.from_store,
                "Cannot remove the leader peer"
            );
            region.clone_with([RegionOption::RemoveStorePeer(s.from_store)])
        }
        _ => panic!("Unknown operator step"),
    }
}

/// Apply through terminal acknowledgement, publish once, then refresh both memberships.
/// `now_unix_nanos` controls only the mock cluster heartbeat time boundary.
pub(crate) fn apply_operator(cluster: &mut BasicCluster, op: &Operator, now_unix_nanos: i64) {
    let origin = cluster
        .regions
        .get_region(op.region_id())
        .expect("operator region does not exist");
    let mut region = (*origin).clone();
    while !op.is_end() {
        region = apply_operator_step(&region, op);
    }
    let region = Arc::new(region);
    let (old, overlaps, changed) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region.clone(), old.as_deref(), &overlaps, changed);
    for id in region.store_ids().into_iter().chain(origin.store_ids()) {
        let store = cluster
            .stores
            .get_store(id)
            .expect("operator store does not exist");
        let used = (store.region_size() as u64).wrapping_mul(1024 * 1024);
        cluster.update_store_status(id);
        let mut store = cluster
            .stores
            .get_store(id)
            .expect("store remains registered")
            .shallow_clone();
        store.set_new_store_stats(StoreStats {
            capacity: 100 * 1024 * 1024 * 1024,
            available: (100u64 * 1024 * 1024 * 1024).wrapping_sub(used),
            used_size: used,
            ..Default::default()
        });
        store.set_last_heartbeat_unix_nanos(now_unix_nanos);
        cluster.stores.put_store(Arc::new(store));
    }
}
