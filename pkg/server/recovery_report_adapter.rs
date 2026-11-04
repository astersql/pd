// Copyright 2026 AsterSQL.
//! Project the fields read by recovery algorithms; storage-engine local metadata remains transport-owned.
use astersql_pd_proto::pdpb;
use astersql_pd_unsaferecovery::{PeerReport, RaftState, StoreReport};
pub fn from_proto(report: pdpb::StoreReport) -> StoreReport {
    StoreReport {
        step: report.step,
        peers: report
            .peer_reports
            .into_iter()
            .map(|peer| PeerReport {
                region: peer
                    .region_state
                    .and_then(|s| crate::core_region_adapter::region_from_proto(s.region)),
                raft: peer.raft_state.map(|s| {
                    let hard = s.hard_state.unwrap_or_default();
                    RaftState {
                        term: hard.term,
                        vote: hard.vote,
                        commit: hard.commit,
                        last_index: s.last_index,
                    }
                }),
                is_force_leader: peer.is_force_leader,
                has_commit_merge: peer.has_commit_merge,
                applied_index: peer.applied_index,
            })
            .collect(),
    }
}
/// Convert a prepared domain plan without assigning a new recovery step.
pub fn plan_to_proto(plan: astersql_pd_unsaferecovery::RecoveryPlan) -> pdpb::RecoveryPlan {
    pdpb::RecoveryPlan {
        creates: plan
            .creates
            .into_iter()
            .map(crate::core_region_adapter::region_to_proto)
            .collect(),
        tombstones: plan.tombstones,
        demotes: plan
            .demotes
            .into_iter()
            .map(|d| pdpb::DemoteFailedVoters {
                region_id: d.region_id,
                failed_voters: d
                    .failed_voters
                    .into_iter()
                    .map(crate::core_region_adapter::peer_to_proto)
                    .collect(),
            })
            .collect(),
        force_leader: plan.force_leader.map(|f| pdpb::ForceLeader {
            failed_stores: f.failed_stores,
            enter_force_leaders: f.enter_force_leaders,
        }),
        step: plan.step,
        ..Default::default()
    }
}
