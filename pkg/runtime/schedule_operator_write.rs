// Copyright 2026 AsterSQL.
//! Host snapshots, fenced allocation and canonical management handler execution.
use super::*;
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_config::{ProviderSnapshot, SharedConfigProvider};
use astersql_pd_schedule_handler::{
    AdminPeerRequest, AdminRangeContext, CreationContext, Handler, OperatorCreation,
    handle_operator_creation,
};
use astersql_pd_schedule_operator::BuilderError;
use astersql_pd_server::http_operator_write::Command;
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
pub(super) async fn write(
    command: Command,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    labels: &astersql_pd_schedule_labeler::RegionLabeler,
    cancel: &CancellationToken,
) -> Result<(u16, Value), String> {
    let mut guard = resources.lock().await;
    let r = guard.as_mut().ok_or("server not started")?;
    r.leader.validate().await.map_err(|e| e.to_string())?;
    let now = tokio::time::Instant::now().into_std();
    let input = match command {
        Command::Delete(id) => {
            let h = Handler::new(Some(&*coordinator));
            let result = if let Some(id) = id {
                h.remove_operator(id, now)
            } else {
                h.remove_operators(now)
            };
            return result
                .map(|_| {
                    (
                        200,
                        json!(if id.is_some() {
                            "The pending operator is canceled."
                        } else {
                            "All pending operators are canceled."
                        }),
                    )
                })
                .map_err(|e| format!("[{}]{}", e.code(), e));
        }
        Command::Create(input) => input,
    };
    let action = OperatorCreation::parse(&input).map_err(|e| e.to_string())?;
    let regions = coordinator.get_cluster().regions.get_regions();
    // Existing peer and split commands need no IDs. Keep their success independent
    // of the allocator; other factories consume a fenced reservation synchronously.
    let width = match &action {
        OperatorCreation::Peer {
            request: AdminPeerRequest::TransferLeader(_) | AdminPeerRequest::RemovePeer(_),
            ..
        }
        | OperatorCreation::Split { .. } => 0,
        OperatorCreation::Peer {
            request: AdminPeerRequest::TransferRegion(roles),
            ..
        } => roles.len(),
        OperatorCreation::Peer { .. } => 1,
        OperatorCreation::Merge { source, target } => [*source, *target]
            .iter()
            .filter_map(|id| coordinator.get_cluster().regions.get_region(*id))
            .map(|r| r.meta().peers.len())
            .sum(),
        OperatorCreation::Scatter { region, .. } => coordinator
            .get_cluster()
            .regions
            .get_region(*region)
            .map_or(0, |r| r.meta().peers.len()),
        OperatorCreation::ScatterRegions { retries, .. } => regions
            .iter()
            .map(|r| r.meta().peers.len())
            .sum::<usize>()
            .saturating_mul(((*retries).clamp(0, 30) + 1) as usize),
    };
    let mut allocated = VecDeque::new();
    if width > 0 {
        let count = u32::try_from(width).map_err(|_| "operator allocation is too large")?;
        let (last, count) = r
            .allocator
            .alloc_count(count)
            .await
            .map_err(|e| e.to_string())?;
        allocated.extend(last - u64::from(count) + 1..=last);
    }
    let config = r.state.lock().unwrap().config.clone();
    let stats = r
        .hot
        .snapshot(0, regions.iter().map(|r| r.meta().id).collect())
        .await
        .map_err(|e| e.to_string())?;
    let hot = regions
        .iter()
        .filter(|r| stats.is_region_hot(r, config.get_hot_region_cache_hits_threshold()))
        .map(|r| r.meta().id)
        .collect();
    let affinity = astersql_pd_schedule_filter::AffinityFilter {
        manager: None,
        labels,
        now: super::now().0 as u64,
    };
    let mut state = r.state.lock().unwrap();
    let mut response = handle_operator_creation(
        Some(coordinator),
        &input,
        CreationContext {
            range: AdminRangeContext {
                manager: &mut state.rules,
                now_unix_nanos: super::now().1 as i128,
                supports_conf_change_v2: true,
                now,
            },
            hot: &hot,
            affinity: Some(&affinity),
            cancellation: cancel,
        },
        || {
            allocated
                .pop_front()
                .ok_or_else(|| BuilderError::Allocation("operator reservation exhausted".into()))
        },
    )
    .await;
    if let Some(error) = response.batch_error.take() {
        response.body["error"] = json!(wire_error(error, &action));
    }
    let body = if let Some(error) = response.error {
        json!(wire_error(error, &action))
    } else if response.body.is_null() {
        json!("The operator is created.")
    } else {
        response.body
    };
    Ok((response.status, body))
}
fn wire_error(
    error: astersql_pd_schedule_handler::CreationError,
    action: &OperatorCreation,
) -> String {
    use astersql_pd_schedule_handler::{
        AdminPeerError as P, AdminRangeError as R, CreationError as C,
    };
    let region = match action {
        OperatorCreation::Scatter { region, .. } => *region,
        _ => 0,
    };
    match error {
        C::ScatterFailure(astersql_pd_schedule_scatter::ScatterBatchFailure::ExistingOperator) => {
            format!("the operator of region {region} already exist")
        }
        C::ScatterFailure(astersql_pd_schedule_scatter::ScatterBatchFailure::NotReplicated) => {
            format!("region {region} is not fully replicated")
        }
        C::ScatterFailure(astersql_pd_schedule_scatter::ScatterBatchFailure::NoLeader) => {
            format!("region {region} has no leader")
        }
        C::ScatterFailure(astersql_pd_schedule_scatter::ScatterBatchFailure::Admission(_)) => {
            "[PD:operator:ErrAddOperator]failed to add operator, maybe already have one".into()
        }
        C::Handler(e) | C::Peer(P::Handler(e)) | C::Range(R::Handler(e)) => {
            format!("[{}]{}", e.code(), e)
        }
        C::Peer(P::RegionNotFound(id)) | C::Range(R::RegionNotFound(id)) => {
            format!("[PD:region:ErrRegionNotFound]region {id} not found")
        }
        C::Peer(P::StoreNotFound(id)) => format!("[PD:core:ErrStoreNotFound]store {id} not found"),
        C::Peer(P::Admission(_)) | C::Range(R::Admission(_)) => {
            "[PD:operator:ErrAddOperator]failed to add operator, maybe already have one".into()
        }
        C::Range(R::NotAdjacent) => {
            "[PD:region:ErrRegionNotAdjacent]two regions are not adjacent".into()
        }
        C::Range(R::AbnormalPeer(id)) => {
            format!("[PD:region:ErrRegionAbnormalPeer]region {id} has abnormal peer")
        }
        C::Peer(P::StoreRemoved(id)) => {
            format!("[PD:core:ErrStoreRemoved]store {id} has been removed")
        }
        C::Peer(P::StoreUnhealthy(id)) => {
            format!("[PD:core:ErrStoreUnhealthy]store {id} is unhealthy")
        }
        C::ScatterBatch(astersql_pd_schedule_scatter::ScatterBatchError::EmptyRegion) => {
            "[PD:scatter:ErrEmptyRegion]empty region".into()
        }
        C::ScatterBatch(astersql_pd_schedule_scatter::ScatterBatchError::RegionNotFound(_)) => {
            "[PD:region:ErrRegionNotFound]region %v not found".into()
        }
        e => e.to_string(),
    }
}
