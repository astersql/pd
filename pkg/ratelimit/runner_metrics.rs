// Copyright 2026 AsterSQL.
//! Cached handles for one host-owned runner's bounded task vocabulary.
use prometheus::{Counter, Gauge, Histogram};
use std::collections::BTreeMap;
#[derive(Clone)]
pub(crate) struct TaskMetrics {
    pub pending: Gauge,
    pub failed: Counter,
    pub succeeded: Counter,
    pub duration: Histogram,
}
pub struct RunnerMetrics {
    pub(crate) waiting: Gauge,
    pub(crate) tasks: BTreeMap<String, TaskMetrics>,
}
impl RunnerMetrics {
    /// Cache labels before starting the runner. Unknown tasks remain unmeasured;
    /// hosts must include every admitted task name in this finite vocabulary.
    pub fn new(scope: &astersql_pd_metricutil::ClassicMetrics, name: &str, tasks: &[&str]) -> Self {
        Self {
            waiting: scope
                .runner_task_max_waiting_duration
                .with_label_values(&[name]),
            tasks: tasks
                .iter()
                .map(|task| {
                    (
                        (*task).to_owned(),
                        TaskMetrics {
                            pending: scope.runner_pending_tasks.with_label_values(&[name, task]),
                            failed: scope.runner_failed_tasks.with_label_values(&[name, task]),
                            succeeded: scope
                                .runner_succeeded_tasks
                                .with_label_values(&[name, task]),
                            duration: scope
                                .runner_task_execution_duration
                                .with_label_values(&[name, task]),
                        },
                    )
                })
                .collect(),
        }
    }
    pub(crate) fn reset(&self) {
        self.waiting.set(0.);
        for metric in self.tasks.values() {
            metric.pending.set(0.);
        }
    }
}
