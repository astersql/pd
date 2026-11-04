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
