// Copyright 2026 AsterSQL.
//! Cached metrics follow canonical operator identity and terminal deduplication.
use astersql_pd_metricutil::ClassicMetrics;
use prometheus::{Counter, Histogram};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub(crate) struct OperatorMetrics {
    scope: Arc<ClassicMetrics>,
    desc: String,
    events: BTreeMap<&'static str, OnceLock<Counter>>,
    finish: OnceLock<Histogram>,
    size: OnceLock<Histogram>,
    exceeded: OnceLock<Counter>,
    steps: Vec<(&'static str, OnceLock<Histogram>)>,
    created: AtomicBool,
}
impl OperatorMetrics {
    pub(crate) fn new(scope: Arc<ClassicMetrics>, op: &crate::Operator) -> Self {
        use crate::OpStep;
        Self {
            scope,
            desc: op.desc().to_owned(),
            events: [
                "check",
                "promote-success",
                "promote-timeout",
                "promote-unexpected",
                "promote-stale",
                "disappear",
                "put",
                "promote-add",
                "exceed-limit",
                "get",
                "check-failed",
                "not-found",
                "epoch-not-match",
                "already-have",
                "unexpected-status",
                "exceed-max-waiting",
                "expired",
                "redundant",
                "unexpected",
                "start",
                "create",
                "remove",
                "finish",
                "replace",
                "expire",
                "timeout",
                "cancel",
            ]
            .into_iter()
            .map(|event| (event, OnceLock::new()))
            .collect(),
            finish: OnceLock::new(),
            size: OnceLock::new(),
            exceeded: OnceLock::new(),
            created: AtomicBool::new(false),
            steps: (0..op.len())
                .map(|i| {
                    (
                        match op.step(i).unwrap() {
                            OpStep::TransferLeader(_) => "TransferLeader",
                            OpStep::AddPeer(_) => "AddPeer",
                            OpStep::AddLearner(_) => "AddLearner",
                            OpStep::PromoteLearner(_) => "PromoteLearner",
                            OpStep::RemovePeer(_) => "RemovePeer",
                            OpStep::BecomeWitness(_) => "BecomeWitness",
                            OpStep::BecomeNonWitness(_) => "BecomeNonWitness",
                            OpStep::BatchSwitchWitness(_) => "BatchSwitchWitness",
                            OpStep::ChangePeerV2Enter(_) => "ChangePeerV2Enter",
                            OpStep::ChangePeerV2Leave(_) => "ChangePeerV2Leave",
                            OpStep::MergeRegion(_) => "MergeRegion",
                            OpStep::SplitRegion(_) => "SplitRegion",
                        },
                        OnceLock::new(),
                    )
                })
                .collect(),
        }
    }
    pub(crate) fn event(&self, event: &'static str) {
        self.events[event]
            .get_or_init(|| {
                self.scope
                    .operator_counter
                    .with_label_values(&[&self.desc, event])
            })
            .inc();
    }
    pub(crate) fn started(&self, size: i64) {
        self.event("start");
        self.size
            .get_or_init(|| {
                self.scope
                    .operator_size_hist
                    .with_label_values(&[&self.desc])
            })
            .observe(size as f64);
    }
    pub(crate) fn created(&self) {
        if !self.created.swap(true, Ordering::Relaxed) {
            self.event("create");
        }
    }
    pub(crate) fn finished(&self, elapsed: Duration) {
        self.finish
            .get_or_init(|| {
                self.scope
                    .operator_duration
                    .with_label_values(&[&self.desc])
            })
            .observe(elapsed.as_secs_f64());
    }
    pub(crate) fn exceeded(&self) {
        self.exceeded
            .get_or_init(|| {
                self.scope
                    .operator_exceeded_store_limit_counter
                    .with_label_values(&[&self.desc])
            })
            .inc();
        self.event("exceed-limit");
    }
    pub(crate) fn step(&self, index: usize, elapsed: Duration) {
        let (name, metric) = &self.steps[index];
        metric
            .get_or_init(|| self.scope.operator_step_duration.with_label_values(&[name]))
            .observe(elapsed.as_secs_f64());
    }
}
