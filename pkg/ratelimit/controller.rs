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
}
impl Controller {
    /// Creates an empty controller with a child of the caller's cancellation.
    pub fn new(parent: &CancellationToken) -> Self {
        Self {
            state: Mutex::new(State::default()),
            cancellation: parent.child_token(),
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
