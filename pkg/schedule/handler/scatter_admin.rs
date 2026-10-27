// Copyright 2026 AsterSQL.
//! Management actions borrowing the coordinator's canonical mutable scatter owner.
use crate::admin_parse::decode_hex;
use crate::{AdminRangeError, CreationContext, CreationError, Handler, HandlerError};
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_operator::BuilderError;
use astersql_pd_schedule_scatter::{ScatterBatchFailure, ScatterBatchOutcome, ScatterBatchRequest};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Domain response; adapters supply the HTTP/JSON representation.
#[derive(Debug, PartialEq, Eq)]
pub struct ScatterRegionsResponse {
    pub processed_percentage: usize,
}
/// Go counts successful Scatter calls, including no-ops and admission rejections.
/// Failure IDs are counted once, even when an ID was also processed.
pub fn build_scatter_regions_resp(
    ops_count: usize,
    failures: &BTreeMap<u64, ScatterBatchFailure>,
) -> ScatterRegionsResponse {
    let failed = failures.len() as u128;
    ScatterRegionsResponse {
        processed_percentage: if failed == 0 {
            100
        } else {
            (100 - 100 * failed / (ops_count as u128 + failed)) as usize
        },
    }
}
/// Single-region management validates hot state before scatter and uses direct admission.
pub async fn add_scatter_region_operator<C, F>(
    coordinator: Option<&mut Coordinator<C>>,
    region: u64,
    group: &str,
    context: CreationContext<'_>,
    mut allocate: F,
) -> Result<(), CreationError>
where
    C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static,
    F: FnMut() -> Result<u64, BuilderError>,
{
    let c = coordinator.ok_or(HandlerError::NotBootstrapped)?;
    let source = c
        .get_cluster()
        .regions
        .get_region(region)
        .ok_or(AdminRangeError::RegionNotFound(region))?;
    if context.hot.contains(&region) {
        return Err(CreationError::Scatter(format!(
            "region {region} is a hot region"
        )));
    }
    let mut scatter = c.get_region_scatterer(
        context.range.manager,
        context.hot,
        context.affinity,
        context.range.now_unix_nanos,
        context.range.supports_conf_change_v2,
    );
    let mut suspect = BTreeSet::new();
    let op = scatter
        .batch
        .scatter_one(&source, group, false, &mut suspect, &mut allocate)
        .await;
    drop(scatter);
    c.get_checker_controller()
        .lock()
        .unwrap()
        .add_pending_processed_regions(false, &suspect.into_iter().collect::<Vec<_>>());
    let op = op?;
    if let Some(op) = op {
        let removed = c
            .get_operator_controller()
            .add_operators(c.get_cluster(), &[Arc::new(op)])
            .map_err(AdminRangeError::Admission)?
            .removed;
        Handler::new(Some(&*c)).archive_removed(&removed, context.range.now)?;
    }
    Ok(())
}
async fn scatter_request<C, F>(
    coordinator: Option<&mut Coordinator<C>>,
    request: ScatterBatchRequest,
    group: &str,
    retries: i64,
    context: CreationContext<'_>,
    allocate: F,
) -> Result<ScatterBatchOutcome, CreationError>
where
    C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static,
    F: FnMut() -> Result<u64, BuilderError>,
{
    let c = coordinator.ok_or(HandlerError::NotBootstrapped)?;
    Ok(c.get_region_scatterer(
        context.range.manager,
        context.hot,
        context.affinity,
        context.range.now_unix_nanos,
        context.range.supports_conf_change_v2,
    )
    .scatter(
        request,
        group,
        retries.clamp(-1, 30) as i32,
        false,
        context.cancellation,
        allocate,
    )
    .await?)
}
/// Both nonempty hex bounds take precedence over IDs. Bootstrap precedes hex parsing.
pub async fn add_scatter_regions_operators<C, F>(
    coordinator: Option<&mut Coordinator<C>>,
    ids: &[u64],
    start: &str,
    end: &str,
    group: &str,
    retries: i64,
    context: CreationContext<'_>,
    allocate: F,
) -> Result<ScatterBatchOutcome, CreationError>
where
    C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static,
    F: FnMut() -> Result<u64, BuilderError>,
{
    let c = coordinator.ok_or(HandlerError::NotBootstrapped)?;
    let request = if !start.is_empty() && !end.is_empty() {
        ScatterBatchRequest::Range {
            start: decode_hex(start)?,
            end: decode_hex(end)?,
        }
    } else {
        ScatterBatchRequest::Ids(ids.to_vec())
    };
    scatter_request(Some(c), request, group, retries, context, allocate).await
}
/// Explicit range accepts empty unbounded keys and decodes before bootstrap checking.
pub async fn scatter_regions_by_range<C, F>(
    coordinator: Option<&mut Coordinator<C>>,
    start: &str,
    end: &str,
    group: &str,
    retries: i64,
    context: CreationContext<'_>,
    allocate: F,
) -> Result<ScatterBatchOutcome, CreationError>
where
    C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static,
    F: FnMut() -> Result<u64, BuilderError>,
{
    let request = ScatterBatchRequest::Range {
        start: decode_hex(start)?,
        end: decode_hex(end)?,
    };
    scatter_request(coordinator, request, group, retries, context, allocate).await
}
/// Explicit ID action retains the per-ID failure map and canonical retry outcome.
pub async fn scatter_regions_by_id<C, F>(
    coordinator: Option<&mut Coordinator<C>>,
    ids: &[u64],
    group: &str,
    retries: i64,
    context: CreationContext<'_>,
    allocate: F,
) -> Result<ScatterBatchOutcome, CreationError>
where
    C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static,
    F: FnMut() -> Result<u64, BuilderError>,
{
    scatter_request(
        coordinator,
        ScatterBatchRequest::Ids(ids.to_vec()),
        group,
        retries,
        context,
        allocate,
    )
    .await
}
