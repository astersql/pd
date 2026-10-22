// Copyright 2026 AsterSQL.
use crate::{DoneFunc, Limiter};
use astersql_pd_errs::TypedError;
use astersql_pd_syncutil::asynchronous::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

/// Ordered configuration operations with Go-compatible status bits.
#[derive(Debug, Clone, Copy)]
pub enum ControllerOption {
    AllowList,
    Concurrency(u64),
    Qps(f64, i64),
    Dimensions {
        qps: f64,
        burst: i64,
        concurrency: u64,
    },
    Init,
}

/// Explicit completion, including a no-op for unconfigured labels.
#[derive(Debug, Clone)]
pub struct ControllerDone(Option<DoneFunc>);
impl ControllerDone {
    /// Completes an admission once across handle clones.
    pub fn done(&self) {
        if let Some(done) = &self.0 {
            done.done();
        }
    }
}
#[derive(Default)]
struct State {
    limiters: HashMap<String, Arc<Limiter>>,
    allow_list: HashSet<String>,
}

/// Independent label limiters using the caller's runtime and cancellation.
pub struct Controller {
    state: Mutex<State>,
    cancellation: CancellationToken,
    metrics: Option<(String, prometheus::GaugeVec)>,
}
impl Controller {
    /// Creates an empty controller with a child of the caller's cancellation.
    pub fn new(parent: &CancellationToken) -> Self {
        Self {
            state: Mutex::new(State::default()),
            cancellation: parent.child_token(),
            metrics: None,
        }
    }
    /// Registers the caller's gauge with labels ordered as API type and label.
    /// The host runs and joins `run_metrics` on its existing runtime.
    pub fn with_metrics(
        parent: &CancellationToken,
        api_type: impl Into<String>,
        registry: &prometheus::Registry,
        gauge: prometheus::GaugeVec,
    ) -> Result<Self, prometheus::Error> {
        // Reject an incompatible gauge before changing the registry.
        use prometheus::core::Collector;
        let labels = gauge.desc()[0].variable_labels.len();
        if labels != 2 {
            return Err(prometheus::Error::InconsistentCardinality {
                expect: 2,
                got: labels,
            });
        }
        registry.register(Box::new(gauge.clone()))?;
        let mut controller = Self::new(parent);
        controller.metrics = Some((api_type.into(), gauge));
        Ok(controller)
    }

    /// Collects configured concurrency every 15 seconds until close or cancellation.
    /// No task is spawned internally; the host owns and joins this future.
    pub async fn run_metrics(&self) {
        let Some((api_type, gauge)) = &self.metrics else {
            return;
        };
        let period = std::time::Duration::from_secs(15);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = self.cancellation.cancelled() => return,
                _ = interval.tick() => {
                    let limiters: Vec<_> = self.state.lock().await.limiters.iter()
                        .map(|(label, limiter)| (label.clone(), limiter.clone())).collect();
                    for (label, limiter) in limiters {
                        let (limit, _) = limiter.concurrency_status().await;
                        gauge.with_label_values(&[api_type, &label]).set(limit as f64);
                    }
                }
            }
        }
    }

    /// Cancels background work, preserving admission and configuration behavior.
    pub fn close(&self) {
        self.cancellation.cancel();
    }
    /// Reports explicit close or inherited cancellation.
    pub fn is_closed(&self) -> bool {
        self.cancellation.is_cancelled()
    }
    /// Applies options in order and combines their Go status bits.
    /// Allow-list insertion preserves any existing limiter.
    pub async fn update(&self, label: &str, options: &[ControllerOption]) -> u32 {
        let mut state = self.state.lock().await;
        let mut status = 0;
        for option in options {
            if matches!(option, ControllerOption::AllowList) {
                state.allow_list.insert(label.to_owned());
                status |= 16;
                continue;
            }
            if state.allow_list.contains(label) {
                status |= 16;
                continue;
            }
            let limiter = state
                .limiters
                .entry(label.to_owned())
                .or_insert_with(|| Arc::new(Limiter::new()));
            status |= match option {
                ControllerOption::Concurrency(limit) => {
                    limiter.update_concurrency_config(*limit).await as u32
                }
                ControllerOption::Qps(limit, burst) => {
                    limiter.update_qps_config(*limit, *burst).await as u32
                }
                ControllerOption::Dimensions {
                    qps,
                    burst,
                    concurrency,
                } => {
                    let qps = limiter.update_qps_config(*qps, *burst).await as u32;
                    qps | limiter.update_concurrency_config(*concurrency).await as u32
                }
                ControllerOption::Init => 2,
                ControllerOption::AllowList => unreachable!(),
            };
        }
        status
    }
    /// Admits unconfigured labels freely and delegates existing labels.
    pub async fn allow(&self, label: &str) -> Result<ControllerDone, TypedError> {
        let limiter = self.state.lock().await.limiters.get(label).cloned();
        match limiter {
            Some(limiter) => limiter.allow().await.map(|d| ControllerDone(Some(d))),
            None => Ok(ControllerDone(None)),
        }
    }
    /// Returns configured concurrency and current admissions, or zeros.
    pub async fn concurrency_status(&self, label: &str) -> (u64, u64) {
        let limiter = self.state.lock().await.limiters.get(label).cloned();
        match limiter {
            Some(limiter) => limiter.concurrency_status().await,
            None => (0, 0),
        }
    }
    /// Returns configured QPS and burst, or zeros.
    pub async fn qps_status(&self, label: &str) -> (f64, i64) {
        let limiter = self.state.lock().await.limiters.get(label).cloned();
        match limiter {
            Some(limiter) => limiter.qps_status().await,
            None => (0.0, 0),
        }
    }
    /// Reports membership in the configuration allow list.
    pub async fn is_in_allow_list(&self, label: &str) -> bool {
        self.state.lock().await.allow_list.contains(label)
    }
}
