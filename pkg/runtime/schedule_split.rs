// Copyright 2026 AsterSQL.
//! Pending split observations are polled without blocking the coordinator or heartbeats.
use super::*;
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_splitter::{SplitRetry, SplitRetryPhase};
use astersql_pd_server::grpc_split_regions::{SplitRequest, SplitResult};
use tokio::sync::oneshot;

pub(super) struct Pending {
    retry: SplitRetry,
    cancellation: CancellationToken,
    start: tokio::time::Instant,
    reply: oneshot::Sender<Result<SplitResult, String>>,
}
impl ClusterHost {
    pub(crate) async fn split_regions(&self, request: SplitRequest) -> Result<SplitResult, String> {
        let (reply, rx) = oneshot::channel();
        let pending = Pending {
            retry: SplitRetry::new(request.keys, request.retries),
            cancellation: request.cancellation,
            start: tokio::time::Instant::now(),
            reply,
        };
        tokio::select! {biased;
            _=self.cancel.cancelled()=>Err("server not started".into()),
            result=async {
                self.schedule_requests.send(super::schedule_scatter::Command::Split(pending)).await.map_err(|_|"scheduling stopped")?;
                rx.await.map_err(|_|"scheduling stopped")?
            }=>result,
        }
    }
}
/// A closed response cancels observation but preserves the splitter's outer
/// retries and final backoff. Term cancellation drops this collection entirely.
pub(super) async fn drive(
    pending: &mut Vec<Pending>,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
) {
    if pending.is_empty() {
        return;
    }
    let mut guard = resources.lock().await;
    let error = match guard.as_ref() {
        Some(r) => r.leader.validate().await.err().map(|e| e.to_string()),
        None => Some("server not started".into()),
    };
    if let Some(error) = error {
        for p in pending.drain(..) {
            let _ = p.reply.send(Err(error.clone()));
        }
        return;
    }
    let r = guard.as_mut().unwrap();
    let cluster = coordinator.get_cluster().clone();
    let admission = coordinator.operator_handle();
    for p in pending.iter_mut() {
        let ms = p.start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        p.retry.advance(
            ms,
            p.reply.is_closed() || p.cancellation.is_cancelled(),
            &cluster.regions,
        );
        if p.retry.phase() == SplitRetryPhase::Ready {
            let mut state = r.state.lock().unwrap();
            let config = state.config.clone();
            let stores = cluster.stores.get_stores();
            let replication = RegionReplicationContext {
                conf: &config,
                stores: &stores,
                manager: std::cell::RefCell::new(&mut state.rules),
                now: now().1 as i128,
            };
            let submissions = p
                .retry
                .start_attempt(ms, &cluster, &admission, &replication, |flag, id| {
                    coordinator
                        .get_checker_controller()
                        .lock()
                        .unwrap()
                        .add_pending_processed_regions(flag, &[id]);
                })
                .unwrap();
            for outcome in submissions.admitted {
                coordinator
                    .get_operator_records()
                    .bury_removed(&outcome.removed, tokio::time::Instant::now().into_std());
                r.schedule_creations
                    .extend(outcome.admitted.iter().map(|op| op.region_id()));
            }
        }
    }
    let mut i = 0;
    while i < pending.len() {
        if pending[i].retry.phase() == SplitRetryPhase::Done {
            let p = pending.swap_remove(i);
            let result = p.retry.result();
            let _ = p.reply.send(Ok(SplitResult {
                percentage: result.percentage as u64,
                ids: result.region_ids,
            }));
        } else {
            i += 1;
        }
    }
}
