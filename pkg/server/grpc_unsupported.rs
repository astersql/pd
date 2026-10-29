// Copyright 2026 AsterSQL.
//! Deprecated classic RPCs are successful no-ops in the pinned Go service.
//! Fixed unsupported GC watches retain Go status text without starting a stream.
//! Deprecated unary calls deliberately ignore request headers, role, and legacy TSO parameters.
use astersql_pd_proto::pdpb;

pub(crate) fn header(cluster_id: u64) -> Option<pdpb::ResponseHeader> {
    Some(pdpb::ResponseHeader {
        cluster_id,
        error: (cluster_id == 0).then(|| pdpb::Error {
            r#type: pdpb::ErrorType::NotBootstrapped as i32,
            message: "cluster id is not ready".into(),
        }),
    })
}

pub(crate) fn watch_gc_safe_point_v2() -> tonic::Status {
    tonic::Status::unimplemented(
        "WatchGCSafePointV2 is obsolete. Poll GetAllKeyspacesGCStates instead if necessary",
    )
}

pub(crate) fn watch_gc_states() -> tonic::Status {
    tonic::Status::unimplemented("WatchGCStates is not supported yet")
}
