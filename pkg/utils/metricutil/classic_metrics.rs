// Copyright 2026 AsterSQL.
//! Classic PD domain metric handles. Registration and lifecycle belong to the host.
use prometheus::{CounterVec, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts, core::Collector};
use std::sync::Arc;
/// Collectors shared by actual library instances, never process-global state.
pub struct ClassicMetrics {
    distribution_children: std::sync::Mutex<
        std::collections::BTreeMap<(u64, bool, &'static str), prometheus::Counter>,
    >,
    store_config_children:
        std::sync::Mutex<std::collections::BTreeMap<(String, &'static str), prometheus::Counter>>,
    allowance: GaugeVec,
    pub halt_scheduling: Gauge,
    manual_allowance: std::sync::OnceLock<Gauge>,
    pub lease_ttl: LeaseTtlMetrics,
    pub keep_alive_response_interval: HistogramVec,
    pub renewal_termination_total: CounterVec,
    pub keep_alive_request_duration: HistogramVec,
    pub keep_alive_tick_interval: HistogramVec,
    pub id_gauge: GaugeVec,
    pub service_member_gauge: GaugeVec,
    pub member_leader_priority_gauge: GaugeVec,
    pub tso_counter: CounterVec,
    pub tso_gauge: GaugeVec,
    pub tso_gap: GaugeVec,
    pub tso_op_duration: HistogramVec,
    pub tso_allocator_role: GaugeVec,
    pub runner_task_max_waiting_duration: GaugeVec,
    pub runner_pending_tasks: GaugeVec,
    pub runner_failed_tasks: CounterVec,
    pub runner_succeeded_tasks: CounterVec,
    pub runner_task_execution_duration: HistogramVec,
    pub operator_step_duration: HistogramVec,
    pub operator_limit_counter: CounterVec,
    pub operator_exceeded_store_limit_counter: CounterVec,
    pub operator_counter: CounterVec,
    pub operator_duration: HistogramVec,
    pub operator_size_hist: HistogramVec,
    pub scatter_distribution_counter: CounterVec,
    pub txn_counter: CounterVec,
    pub txn_duration: HistogramVec,
    pub etcd_state_gauge: GaugeVec,
    pub etcd_endpoint_latency: HistogramVec,
    pub region_event_counter: CounterVec,
    pub update_store_stats_gauge: Gauge,
    pub cluster_state_c_p_u_gauge: Gauge,
    pub cluster_state_current: GaugeVec,
    pub store_sync_config_event: CounterVec,
    pub store_trigger_network_slow_evict: CounterVec,
    pub raft_cluster_start_duration: HistogramVec,
}
impl ClassicMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let allowance = GaugeVec::new(
            Opts::new(
                "pd_scheduling_allowance_status",
                "Status of the scheduling allowance.",
            ),
            &["kind"],
        )?;
        let halt_scheduling = allowance.with_label_values(&["halt-scheduling"]);
        Ok(Self {
            allowance,halt_scheduling,manual_allowance:Default::default(),
            distribution_children:Default::default(),store_config_children:Default::default(),
            lease_ttl: LeaseTtlMetrics::new()?,
            keep_alive_response_interval: HistogramVec::new(HistogramOpts::new("pd_lease_keepalive_response_interval_seconds","Interval between consecutive valid lease keepalive responses received from etcd, by purpose.").buckets(prometheus::exponential_buckets(0.001, 2.0, 16)?),&["purpose"])?,
            renewal_termination_total: CounterVec::new(Opts::new("pd_lease_renewal_termination_total","Number of lease renewal stop or anomaly events, broken down by purpose and reason. Reason `context_canceled` is benign (caller-initiated shutdown); `lease_expired` indicates keepalive timeout; `invalid_ttl` indicates a successful keepalive response with non-positive TTL."),&["purpose","reason"])?,
            keep_alive_request_duration: HistogramVec::new(HistogramOpts::new("pd_lease_keepalive_request_duration_seconds","Duration of etcd Lease.KeepAliveOnce requests observed by PD, by purpose and result.").buckets(prometheus::exponential_buckets(0.01, 2.0, 13)?),&["purpose","result"])?,
            keep_alive_tick_interval: HistogramVec::new(HistogramOpts::new("pd_lease_keepalive_tick_interval_seconds","Interval between consecutive KeepAliveOnce call start times, observed inside each worker goroutine by purpose. Spikes indicate local scheduling delay (CPU starvation, GC pauses, goroutine pressure) and correlate with the `the interval between keeping alive lease is too long` warning.").buckets(prometheus::exponential_buckets(0.01, 2.0, 13)?),&["purpose"])?,
            id_gauge: GaugeVec::new(Opts::new("pd_cluster_id","Record of id allocator."),&["type"])?,
            service_member_gauge: GaugeVec::new(Opts::new("service_member_role","The leader/primary of services"),&["service"])?,
            member_leader_priority_gauge: GaugeVec::new(Opts::new("pd_server_member_leader_priority","Etcd leader priority of this PD instance."),&[])?,
            tso_counter: CounterVec::new(Opts::new("pd_tso_events","Counter of tso events"),&["type","group"])?,
            tso_gauge: GaugeVec::new(Opts::new("pd_cluster_tso","Record of tso metadata."),&["type","group"])?,
            tso_gap: GaugeVec::new(Opts::new("pd_cluster_tso_gap_millionseconds","The minimal (non-zero) TSO gap"),&["group"])?,
            tso_op_duration: HistogramVec::new(HistogramOpts::new("pd_cluster_tso_operation_duration_seconds","Bucketed histogram of processing time(s) of the TSO operations.").buckets(prometheus::exponential_buckets(0.0005, 2.0, 13)?),&["type","group"])?,
            tso_allocator_role: GaugeVec::new(Opts::new("pd_tso_role","Indicate the PD server role info, whether it's a TSO allocator."),&["group"])?,
            runner_task_max_waiting_duration: GaugeVec::new(Opts::new("pd_ratelimit_runner_task_max_waiting_duration_seconds","The duration of tasks waiting in the runner."),&["runner_name"])?,
            runner_pending_tasks: GaugeVec::new(Opts::new("pd_ratelimit_runner_pending_tasks","The number of pending tasks in the runner."),&["runner_name","task_type"])?,
            runner_failed_tasks: CounterVec::new(Opts::new("pd_ratelimit_runner_failed_tasks_total","The number of failed tasks in the runner."),&["runner_name","task_type"])?,
            runner_succeeded_tasks: CounterVec::new(Opts::new("pd_ratelimit_runner_success_tasks_total","The number of tasks in the runner."),&["runner_name","task_type"])?,
            runner_task_execution_duration: HistogramVec::new(HistogramOpts::new("pd_ratelimit_runner_task_execution_duration_seconds","Bucketed histogram of processing time (s) of finished tasks.").buckets(prometheus::exponential_buckets(0.0005, 2.0, 13)?),&["runner_name","task_type"])?,
            operator_step_duration: HistogramVec::new(HistogramOpts::new("pd_schedule_finish_operator_steps_duration_seconds","Bucketed histogram of processing time (s) of finished operator step.").buckets(vec![0.5,1.0,2.0,4.0,8.0,16.0,20.0,40.0,60.0,90.0,120.0,180.0,240.0,300.0,480.0,600.0,720.0,900.0,1200.0,1800.0,3600.0]),&["type"])?,
            operator_limit_counter: CounterVec::new(Opts::new("pd_schedule_operator_limit","Counter of operator meeting limit"),&["type","name"])?,
            operator_exceeded_store_limit_counter: CounterVec::new(Opts::new("pd_schedule_operator_exceeded_store_limit","Counter of operator meeting store limit"),&["desc"])?,
            operator_counter: CounterVec::new(Opts::new("pd_schedule_operators_count","Counter of schedule operators."),&["type","event"])?,
            operator_duration: HistogramVec::new(HistogramOpts::new("pd_schedule_finish_operators_duration_seconds","Bucketed histogram of processing time (s) of finished operator.").buckets(vec![0.5,1.0,2.0,4.0,8.0,16.0,20.0,40.0,60.0,90.0,120.0,180.0,240.0,300.0,480.0,600.0,720.0,900.0,1200.0,1800.0,3600.0]),&["type"])?,
            operator_size_hist: HistogramVec::new(HistogramOpts::new("pd_schedule_operator_region_size","Bucketed histogram of the operator region size.").buckets(prometheus::exponential_buckets(1.0, 2.0, 20)?),&["type"])?,
            scatter_distribution_counter: CounterVec::new(Opts::new("pd_schedule_scatter_distribution","Counter of the distribution in scatter."),&["store","is_leader","engine"])?,
            txn_counter: CounterVec::new(Opts::new("pd_txn_txns_count","Counter of txns."),&["result"])?,
            txn_duration: HistogramVec::new(HistogramOpts::new("pd_txn_handle_txns_duration_seconds","Bucketed histogram of processing time (s) of handled txns.").buckets(prometheus::exponential_buckets(0.001, 2.0, 17)?),&["result"])?,
            etcd_state_gauge: GaugeVec::new(Opts::new("pd_server_etcd_client","Etcd client states."),&["source","type"])?,
            etcd_endpoint_latency: HistogramVec::new(HistogramOpts::new("pd_server_etcd_endpoint_latency_seconds","Bucketed histogram of latency of health check.").buckets(vec![1.0,2.0,3.0,4.0,5.0,6.0,7.0,8.0,9.0,10.0]),&["source","endpoint"])?,
            region_event_counter: CounterVec::new(Opts::new("pd_cluster_region_event","Counter of the region event"),&["event"])?,
            update_store_stats_gauge: Gauge::with_opts(Opts::new("pd_cluster_update_stores_stats_time","Time spent of updating store stats."))?,
            cluster_state_c_p_u_gauge: Gauge::with_opts(Opts::new("pd_server_cluster_state_cpu_usage","CPU usage to determine the cluster state"))?,
            cluster_state_current: GaugeVec::new(Opts::new("pd_server_cluster_state_current","Current state of the cluster"),&["state"])?,
            store_sync_config_event: CounterVec::new(Opts::new("pd_cluster_store_sync","The state of store sync config"),&["address","state"])?,
            store_trigger_network_slow_evict: CounterVec::new(Opts::new("pd_cluster_store_trigger_network_slow_evict","The count of store trigger network slow evict"),&["store"])?,
            raft_cluster_start_duration: HistogramVec::new(HistogramOpts::new("pd_cluster_raftcluster_start_duration_seconds","Bucketed histogram of processing time (s) of raft cluster start.").buckets(prometheus::exponential_buckets(0.001, 2.0, 13)?),&["type"])?,
        })
    }
    pub fn scheduling_metrics(&self, halt: bool) {
        let value = if halt { 1. } else { 0. };
        self.halt_scheduling.set(value);
        self.manual_allowance
            .get_or_init(|| self.allowance.with_label_values(&["manually"]))
            .set(value);
    }
    pub fn store_config_metrics(&self, address: &str, state: &'static str) {
        self.store_config_children
            .lock()
            .unwrap()
            .entry((address.to_owned(), state))
            .or_insert_with(|| {
                self.store_sync_config_event
                    .with_label_values(&[address, state])
            })
            .inc();
    }
    pub fn scatter_metrics(&self, id: u64, leader: bool, engine: &str) {
        let engine = match engine {
            "tikv" => "tikv",
            "tiflash" => "tiflash",
            _ => return,
        };
        self.distribution_children
            .lock()
            .unwrap()
            .entry((id, leader, engine))
            .or_insert_with(|| {
                self.scatter_distribution_counter.with_label_values(&[
                    id.to_string().as_str(),
                    if leader { "true" } else { "false" },
                    engine,
                ])
            })
            .inc();
    }
    pub fn delete_scatter_metrics(&self, id: u64) {
        self.distribution_children
            .lock()
            .unwrap()
            .retain(|(store, leader, engine), _| {
                if *store != id {
                    return true;
                }
                let _ = self.scatter_distribution_counter.remove_label_values(&[
                    id.to_string().as_str(),
                    if *leader { "true" } else { "false" },
                    engine,
                ]);
                false
            });
    }
    pub fn collectors(&self) -> Vec<Box<dyn Collector>> {
        vec![
            Box::new(self.allowance.clone()),
            Box::new(self.lease_ttl.clone()),
            Box::new(self.keep_alive_response_interval.clone()),
            Box::new(self.renewal_termination_total.clone()),
            Box::new(self.keep_alive_request_duration.clone()),
            Box::new(self.keep_alive_tick_interval.clone()),
            Box::new(self.id_gauge.clone()),
            Box::new(self.service_member_gauge.clone()),
            Box::new(self.member_leader_priority_gauge.clone()),
            Box::new(self.tso_counter.clone()),
            Box::new(self.tso_gauge.clone()),
            Box::new(self.tso_gap.clone()),
            Box::new(self.tso_op_duration.clone()),
            Box::new(self.tso_allocator_role.clone()),
            Box::new(self.runner_task_max_waiting_duration.clone()),
            Box::new(self.runner_pending_tasks.clone()),
            Box::new(self.runner_failed_tasks.clone()),
            Box::new(self.runner_succeeded_tasks.clone()),
            Box::new(self.runner_task_execution_duration.clone()),
            Box::new(self.operator_step_duration.clone()),
            Box::new(self.operator_limit_counter.clone()),
            Box::new(self.operator_exceeded_store_limit_counter.clone()),
            Box::new(self.operator_counter.clone()),
            Box::new(self.operator_duration.clone()),
            Box::new(self.operator_size_hist.clone()),
            Box::new(self.scatter_distribution_counter.clone()),
            Box::new(self.txn_counter.clone()),
            Box::new(self.txn_duration.clone()),
            Box::new(self.etcd_state_gauge.clone()),
            Box::new(self.etcd_endpoint_latency.clone()),
            Box::new(self.region_event_counter.clone()),
            Box::new(self.update_store_stats_gauge.clone()),
            Box::new(self.cluster_state_c_p_u_gauge.clone()),
            Box::new(self.cluster_state_current.clone()),
            Box::new(self.store_sync_config_event.clone()),
            Box::new(self.store_trigger_network_slow_evict.clone()),
            Box::new(self.raft_cluster_start_duration.clone()),
        ]
    }
}

/// Scrape-time local TTL, matching the active term's monotonic clock.
#[derive(Clone)]
pub struct LeaseTtlMetrics {
    gauge: GaugeVec,
    remaining: Arc<
        std::sync::Mutex<
            std::collections::BTreeMap<
                &'static str,
                std::sync::Weak<dyn Fn() -> f64 + Send + Sync>,
            >,
        >,
    >,
}
impl LeaseTtlMetrics {
    fn new() -> Result<Self, prometheus::Error> {
        Ok(Self {
            gauge: GaugeVec::new(
                Opts::new(
                    "pd_lease_local_ttl_remaining_seconds",
                    "PD local estimate of remaining lease TTL, computed at scrape time as time.Until(expireTime), clamped to 0.",
                ),
                &["purpose"],
            )?,
            remaining: Default::default(),
        })
    }
    pub fn bind(
        &self,
        purpose: &'static str,
        remaining: std::sync::Weak<dyn Fn() -> f64 + Send + Sync>,
    ) {
        self.remaining.lock().unwrap().insert(purpose, remaining);
    }
}
impl Collector for LeaseTtlMetrics {
    fn desc(&self) -> Vec<&prometheus::core::Desc> {
        self.gauge.desc()
    }
    fn collect(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.remaining.lock().unwrap().retain(|purpose, remaining| {
            if let Some(read) = remaining.upgrade() {
                self.gauge.with_label_values(&[purpose]).set(read().max(0.));
                true
            } else {
                let _ = self.gauge.remove_label_values(&[purpose]);
                false
            }
        });
        self.gauge.collect()
    }
}
