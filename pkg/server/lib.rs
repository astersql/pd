// Copyright 2026 AsterSQL.
//! HTTP and gRPC adapters for the minimal classic PD role.
mod grpc;
mod grpc_alloc_id;
pub mod grpc_ask_split;
mod grpc_bootstrap;
mod grpc_cluster_config;
mod grpc_external_ts;
mod grpc_global_config;
mod grpc_global_watch;
mod grpc_members;
mod grpc_region_lookup;
mod grpc_region_query;
mod grpc_region_scan;
mod grpc_report_split;
mod grpc_resolved_ts;
pub mod grpc_scatter;
pub mod grpc_split_regions;
mod grpc_split_scatter;
pub mod grpc_store;
mod grpc_tso;
mod grpc_unsupported;
mod http;
pub mod http_admin_cache;
pub mod http_admin_markers;
pub mod http_admin_recover_id;
pub mod http_cluster;
mod http_config_read;
pub mod http_contract;
pub mod http_health;
mod http_leader;
pub mod http_member;
pub mod http_region_schedule;
pub mod http_resolved_ts;
pub mod http_rule;
mod http_service_limit;
#[cfg(test)]
#[path = "http_service_limit_test.rs"]
mod http_service_limit_test;
mod http_store_read;
mod http_v2_boundary;
#[cfg(test)]
mod http_v2_boundary_test;
mod request_role;
use astersql_pd_tso::Timestamp;
use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Adapter-independent local identity. Discovery reads the shared member registry.
#[derive(Clone)]
pub struct Identity {
    pub cluster_id: u64,
    pub member_id: u64,
    pub name: String,
    pub client_url: String,
    pub peer_url: String,
}
/// Domain operations supplied by the host; transport types stay in this crate.
#[async_trait]
pub trait PdServices: Send + Sync {
    /// Shared authoritative store, available independently of leader/bootstrap state.
    fn recovery_marker_store(&self) -> Option<Arc<dyn astersql_pd_storage::MetaStore>> {
        None
    }
    /// Reset the active term's canonical timestamp oracle.
    async fn reset_timestamp(&self, _tso: u64, _force: bool) -> Result<(), String> {
        Err("[PD:server:ErrServerNotStarted]server not started".into())
    }
    /// Reset the active term's canonical persistent ID allocator.
    async fn recover_alloc_id(&self, _id: u64) -> Result<(), String> {
        Err("server not started".into())
    }
    /// Mutate the existing region cache or storage under host admission.
    async fn admin_region(
        &self,
        _command: http_admin_cache::Command,
        _follower: bool,
    ) -> Result<(u16, serde_json::Value), String> {
        Err("server not started".into())
    }
    /// Read affinity state from the active term's canonical manager.
    async fn affinity_read(
        &self,
        _request: http_affinity_read::Request,
    ) -> Result<(u16, serde_json::Value), String> {
        Err("server not started".into())
    }

    /// Placement management on the existing term coordinator.
    /// Access the active term's region-label rules and region label snapshots.
    async fn region_label_rules(
        &self,
        _request: http_region_label::Request,
    ) -> Result<(u16, serde_json::Value), String> {
        Err("region labels are not configured".into())
    }
    async fn rule(&self, _request: http_rule::Request) -> Result<(u16, serde_json::Value), String> {
        Err("server not started".into())
    }

    /// Accelerate the checker pending queue on the active term coordinator.
    async fn accelerate_regions(
        &self,
        _request: http_region_schedule::AccelerateRequest,
    ) -> Result<(), String> {
        Err("server not started".into())
    }

    /// Inspect or mutate the host-owned checker and diagnostic state.
    async fn checker(&self, _command: http_checker::Command) -> Result<serde_json::Value, String> {
        Err("server not started".into())
    }

    /// Dispatch a scheduler-relative request on the live coordinator.
    async fn scheduler_config(
        &self,
        _request: axum::http::Request<axum::body::Body>,
    ) -> Result<axum::response::Response, String> {
        Err("server not started".into())
    }
    /// Manage the live coordinator scheduler instances.
    async fn scheduler(
        &self,
        _command: http_scheduler::Command,
    ) -> Result<(u16, serde_json::Value), String> {
        Err("server not started".into())
    }
    /// Mutate canonical operators through the host scheduling queue.
    async fn operator_write(
        &self,
        _command: http_operator_write::Command,
    ) -> Result<(u16, serde_json::Value), String> {
        Err("server not started".into())
    }
    /// Read canonical running and retained operators through the host queue.
    async fn operator_read(
        &self,
        _query: http_operator_read::Query,
    ) -> Result<serde_json::Value, String> {
        Err("server not started".into())
    }
    fn identity(&self) -> &Identity;
    /// Host configuration snapshot; defaults do not include live dynamic overrides.
    async fn configuration(
        &self,
        _defaults: bool,
    ) -> Result<astersql_pd_server_config::Config, String> {
        Err("server not started".into())
    }
    /// Persist and publish a PD server field or cluster version on the active term.
    async fn patch_server_config(
        &self,
        _key: String,
        _value: serde_json::Value,
    ) -> Result<(), String> {
        Err("server not started".into())
    }
    /// Apply replication or label-property changes in the host coordinator.
    async fn patch_replication(
        &self,
        _labels: bool,
        _patch: serde_json::Value,
    ) -> Result<(), String> {
        Err("server not started".into())
    }
    /// Apply a partial schedule update through the active coordinator.
    async fn patch_schedule(&self, _patch: serde_json::Value) -> Result<(), String> {
        Err("server not started".into())
    }
    async fn cluster_observation(&self) -> Result<http_cluster::ClusterObservation, String> {
        Err("server not started".into())
    }
    async fn member_metadata(&self, _id: u64) -> http_member::MemberMetadata {
        Default::default()
    }
    async fn set_member_priority(&self, _id: u64, _priority: i64) -> Result<(), String> {
        Err("member priority is not configured".into())
    }
    /// Transfer consensus leadership through explicit host-owned membership.
    async fn transfer_leader(&self, _target: Option<u64>) -> Result<(), String> {
        Err("dedicated member ownership is not configured; shared backend transfer refused".into())
    }
    async fn remove_member(&self, _id: u64) -> Result<(), String> {
        Err("dedicated member ownership is not configured".into())
    }

    /// Optional store state service supplied by the host.
    fn store_heartbeat_service(
        &self,
    ) -> Option<&dyn store_heartbeat_adapter::StoreHeartbeatService> {
        None
    }
    /// Optional region state service supplied by the host.
    fn region_heartbeat_service(
        &self,
    ) -> Option<&dyn region_heartbeat_adapter::RegionHeartbeatService> {
        None
    }
    /// Optional bucket state service supplied by the host.
    fn bucket_heartbeat_service(
        &self,
    ) -> Option<&dyn bucket_heartbeat_adapter::BucketHeartbeatService> {
        None
    }
    /// Optional host-owned region synchronization transport.
    fn region_sync_service(&self) -> Option<&sync_transport::RegionSyncer> {
        None
    }
    /// Owned term handle for hosts whose leader service can change concurrently.
    fn region_sync_handle(&self) -> Option<Arc<sync_transport::RegionSyncer>> {
        None
    }
    /// Canonical metadata reads used by startup recovery verification.
    fn cluster_read_service(&self) -> Option<&dyn ClusterReadService> {
        None
    }
    /// Store configuration gate, matching Go's disabled-by-default region buckets.
    fn region_buckets_enabled(&self) -> bool {
        false
    }
    /// Local synchronized follower view, separate from the scheduling leader term.
    fn follower_read_service(&self) -> Option<&dyn ClusterReadService> {
        None
    }
    /// Whether this host currently exposes a bootstrapped raft cluster.
    /// Implementations must read canonical state, rather than infer it from requests.
    async fn is_bootstrapped(&self) -> Result<bool, String> {
        Ok(true)
    }
    /// Shared canonical discovery metadata, independent of the local leader term.
    fn discovery_store(&self) -> Option<Arc<dyn astersql_pd_storage::MetaStore>> {
        None
    }
    /// Metadata backend fenced to the active leader term for configuration writes.
    fn cluster_config_store(&self) -> Option<Arc<dyn astersql_pd_storage::MetaStore>> {
        self.discovery_store()
    }
    /// Commit initial metadata and publish it into the canonical host cache.
    async fn bootstrap(
        &self,
        _store: astersql_pd_core::StoreMeta,
        _region: astersql_pd_core::RegionMeta,
    ) -> Result<bool, String> {
        Err("bootstrap is not configured".into())
    }
    /// Read the host-owned external timestamp restored from canonical storage.
    async fn external_timestamp(&self) -> Result<u64, String> {
        Err("external timestamp is not configured".into())
    }
    /// Validate monotonicity, publish in memory, then persist, matching Go ordering.
    async fn set_external_timestamp(&self, _timestamp: u64) -> Result<(), String> {
        Err("external timestamp is not configured".into())
    }
    /// Read the serving generation resolved timestamp snapshot.
    async fn resolved_ts_observation(&self) -> Result<http_resolved_ts::Observation, String> {
        Err("server not started".into())
    }
    /// Apply a report to the canonical store cache; aggregation persists separately.
    async fn report_min_resolved_ts(&self, _store_id: u64, _ts: u64) -> Result<(), String> {
        Err("resolved timestamp reports are not configured".into())
    }
    /// Persist registration into the host-owned canonical cluster.
    async fn put_store(
        &self,
        _store: astersql_pd_core::StoreMeta,
    ) -> Result<(), grpc_store::StoreError> {
        Err(grpc_store::StoreError::Other(
            "store registration is not configured".into(),
        ))
    }
    /// Apply a lifecycle command through the host admission and persistence boundary.
    async fn mutate_store(
        &self,
        _operation: http_store_state::StoreMutation,
    ) -> Result<(), String> {
        Err("store lifecycle is not configured".into())
    }
    /// Read the active store progress maintained by the host worker.
    async fn store_progress(
        &self,
        _id: Option<u64>,
        _action: String,
    ) -> Result<serde_json::Value, String> {
        Err("progress manager is not initialized".into())
    }
    /// List metadata from the same view used by heartbeats and GetStore.
    async fn stores(&self) -> Result<Vec<astersql_pd_core::StoreMeta>, String> {
        Err("store listing is not configured".into())
    }
    async fn split_regions(
        &self,
        _request: grpc_split_regions::SplitRequest,
    ) -> Result<grpc_split_regions::SplitResult, String> {
        Err("splitting is not configured".into())
    }
    async fn scatter(
        &self,
        _request: grpc_scatter::ScatterRequest,
    ) -> Result<grpc_scatter::ScatterResult, grpc_scatter::ScatterError> {
        Err(grpc_scatter::ScatterError::Other(
            "scheduling is not configured".into(),
        ))
    }
    /// Read one label from the active coordinator's canonical labeler.
    async fn region_label(
        &self,
        _region: Arc<astersql_pd_core::RegionInfo>,
        _key: String,
    ) -> Result<String, String> {
        Err("region labels are not configured".into())
    }
    async fn operator(&self, _id: u64) -> Result<Option<grpc_scatter::OperatorResult>, String> {
        Err("scheduling is not configured".into())
    }
    /// Whether the host has started polling its listener and still admits probes.
    fn probe_listener_ready(&self) -> bool {
        false
    }
    /// Inspect this host's own consensus member, never an arbitrary external member.
    async fn local_consensus_probe(
        &self,
        _probe: http_health::LocalConsensusProbe,
    ) -> Result<(), String> {
        Err("local consensus diagnostics are not configured".into())
    }
    /// Local listener and cluster initialization; unsupported hosts fail closed.
    async fn probe_status(&self) -> Result<http_health::ReadyStatus, String> {
        Err("local readiness is not configured".into())
    }
    /// Read this member's durable hot-region history, including before bootstrap.
    async fn hotspot_history(
        &self,
        _query: astersql_pd_storage::HistoryQuery,
    ) -> Result<Option<Vec<astersql_pd_storage::HistoryHotRegion>>, String> {
        Err("hot-region history is not configured".into())
    }
    async fn healthy(&self) -> Result<(), String>;
    async fn alloc_id(&self) -> Result<u64, String>;
    /// Reserve one indivisible batch and return its last ID and count.
    async fn alloc_ids(&self, count: u32) -> Result<(u64, u32), String> {
        if count != 1 {
            return Err("batch ID allocation is not configured".into());
        }
        self.alloc_id().await.map(|id| (id, 1))
    }
    /// An unsupported cluster version, or None when batch splitting is supported.
    async fn batch_split_version(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
    /// Validate and allocate against the same term-owned cluster state.
    async fn split_ids(
        &self,
        _region: astersql_pd_core::RegionMeta,
        _count: u32,
        _batch: bool,
        _reason: i32,
    ) -> Result<grpc_ask_split::SplitIds, grpc_ask_split::SplitError> {
        Err(grpc_ask_split::SplitError::Other(
            "split allocation is not configured".into(),
        ))
    }
    async fn timestamp(&self, count: u32) -> Result<Timestamp, String>;
}
/// Host-owned metadata lookup; protocol conversion remains in the server adapter.
#[async_trait]
pub trait ClusterReadService: Send + Sync {
    /// Observe live hotspot windows through the serving term.
    async fn hotspot(&self, _query: http_hotspot::Query) -> Result<serde_json::Value, String> {
        Err("hotspot statistics are not configured".into())
    }

    /// Read the heartbeat-owned anomaly index against current Region snapshots.
    async fn region_anomalies(
        &self,
        _kind: astersql_pd_statistics::RegionStatisticType,
    ) -> Result<Vec<Arc<astersql_pd_core::RegionInfo>>, String> {
        Err("region checks are not configured".into())
    }
    async fn region_range_holes(&self) -> Result<Vec<[String; 2]>, String> {
        Err("range holes are not configured".into())
    }
    /// Check fits and pending membership on the serving coordinator.
    async fn regions_replicated(&self, _start: &str, _end: &str) -> Result<String, String> {
        Err("replication checks are not configured".into())
    }

    /// Aggregate canonical range and hot-cache snapshots without transport types.
    async fn region_statistics(
        &self,
        _start: &[u8],
        _end: &[u8],
        _count: bool,
        _hot: bool,
        _engine: &str,
    ) -> Result<astersql_pd_statistics::RegionStats, String> {
        Err("region statistics are not configured".into())
    }

    /// Snapshot the canonical root or store role index for HTTP list encoding.
    async fn region_list(
        &self,
        _store: Option<(u64, Option<astersql_pd_core::RegionRole>)>,
    ) -> Result<Vec<Arc<astersql_pd_core::RegionInfo>>, String> {
        Err("region lists are not configured".into())
    }
    async fn region_count(&self) -> Result<usize, String> {
        Err("region count is not configured".into())
    }
    /// Read touching neighbors from one canonical cache snapshot.
    async fn adjacent_regions(
        &self,
        _region: &astersql_pd_core::RegionInfo,
    ) -> Result<
        (
            Option<Arc<astersql_pd_core::RegionInfo>>,
            Option<Arc<astersql_pd_core::RegionInfo>>,
        ),
        String,
    > {
        Err("adjacent regions are not configured".into())
    }

    /// None means the follower synchronization worker is not running.
    async fn query_regions(
        &self,
        _keys: &[Vec<u8>],
        _prev_keys: &[Vec<u8>],
        _ids: &[u64],
        _need_buckets: bool,
    ) -> Result<Option<astersql_pd_core::RegionQueryResult>, String> {
        Err("region queries are not configured".into())
    }

    async fn batch_scan_regions(
        &self,
        _ranges: &[astersql_pd_core::KeyRange],
        _options: astersql_pd_core::BatchScanRegionsOptions,
    ) -> Result<
        Result<Vec<Arc<astersql_pd_core::RegionInfo>>, astersql_pd_core::RegionNotAdjacent>,
        String,
    > {
        Err("batch scan is not configured".into())
    }
    async fn store(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::StoreInfo>>, String>;
    async fn region(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::RegionInfo>>, String>;
    async fn scan_regions(
        &self,
        _start: &[u8],
        _end: &[u8],
        _limit: isize,
    ) -> Result<Vec<Arc<astersql_pd_core::RegionInfo>>, String> {
        Err("region scan is not configured".into())
    }
    async fn region_by_key(
        &self,
        _key: &[u8],
        _previous: bool,
    ) -> Result<Option<Arc<astersql_pd_core::RegionInfo>>, String> {
        Err("region key lookup is not configured".into())
    }
}
/// One router, listener and cancellation scope for HTTP/1 and HTTP/2 gRPC.
pub fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> axum::Router {
    let limits = http_service_limit::ServiceLimit::new(services.clone(), &cancel);
    tonic::service::Routes::new(astersql_pd_proto::pdpb::pd_server::PdServer::new(
        sync_transport::SyncGrpc(grpc::Grpc {
            services: services.clone(),
            cancel: cancel.clone(),
        }),
    ))
    .into_axum_router()
    .merge(http::router(services.clone(), cancel.clone()))
    .merge(http_health::router(services.clone(), cancel.clone()))
    .merge(http_service_limit::router(limits.clone()))
    .layer(axum::middleware::from_fn_with_state(
        limits,
        http_service_limit::middleware,
    ))
    .layer(axum::middleware::from_fn_with_state(
        http_redirect::RedirectState {
            services: services.clone(),
            cancel: cancel.clone(),
        },
        grpc_forward::middleware,
    ))
    .layer(axum::middleware::from_fn_with_state(
        http_redirect::RedirectState {
            services,
            cancel: cancel.clone(),
        },
        http_redirect::middleware,
    ))
    .layer(axum::middleware::from_fn_with_state(
        request_context::ContextState::new(cancel),
        request_context::middleware,
    ))
}

pub mod core_trace;
#[cfg(test)]
#[path = "core_trace_test.rs"]
mod core_trace_test;

pub mod core_store_adapter;
#[cfg(test)]
#[path = "core_store_adapter_test.rs"]
mod core_store_adapter_test;

pub mod core_region_adapter;
#[cfg(test)]
#[path = "core_region_adapter_test.rs"]
mod core_region_adapter_test;

pub mod store_heartbeat_adapter;
#[cfg(test)]
#[path = "store_heartbeat_adapter_test.rs"]
mod store_heartbeat_adapter_test;

pub mod region_heartbeat_adapter;
#[cfg(test)]
#[path = "region_heartbeat_adapter_test.rs"]
mod region_heartbeat_adapter_test;

pub mod bucket_heartbeat_adapter;
#[cfg(test)]
#[path = "bucket_heartbeat_adapter_test.rs"]
mod bucket_heartbeat_adapter_test;

pub mod sync_codec;
#[cfg(test)]
#[path = "sync_codec_test.rs"]
mod sync_codec_test;

pub mod sync_transport;
#[cfg(test)]
#[path = "sync_transport_test.rs"]
mod sync_transport_test;

pub mod schedule_wire;
#[cfg(test)]
#[path = "schedule_wire_test.rs"]
mod schedule_wire_test;

pub mod schedule_admin_balance_leader;
#[cfg(test)]
#[path = "schedule_admin_balance_leader_test.rs"]
mod schedule_admin_balance_leader_test;

pub mod schedule_admin_balance_region;
#[cfg(test)]
#[path = "schedule_admin_balance_region_test.rs"]
mod schedule_admin_balance_region_test;

pub mod schedule_admin_balance_witness;
#[cfg(test)]
#[path = "schedule_admin_balance_witness_test.rs"]
mod schedule_admin_balance_witness_test;

pub mod schedule_admin_evict_leader;
#[cfg(test)]
#[path = "schedule_admin_evict_leader_test.rs"]
mod schedule_admin_evict_leader_test;

pub mod schedule_admin_evict_slow_store;
#[cfg(test)]
#[path = "schedule_admin_evict_slow_store_test.rs"]
mod schedule_admin_evict_slow_store_test;

pub mod schedule_admin_evict_stopping_store;
#[cfg(test)]
#[path = "schedule_admin_evict_stopping_store_test.rs"]
mod schedule_admin_evict_stopping_store_test;

pub mod schedule_admin_grant_hot_region;
#[cfg(test)]
#[path = "schedule_admin_grant_hot_region_test.rs"]
mod schedule_admin_grant_hot_region_test;

pub mod schedule_admin_hot_region;
#[cfg(test)]
#[path = "schedule_admin_hot_region_test.rs"]
mod schedule_admin_hot_region_test;

pub mod schedule_admin_grant_leader;
#[cfg(test)]
#[path = "schedule_admin_grant_leader_test.rs"]
mod schedule_admin_grant_leader_test;

// Label scheduler inherited configuration endpoint.
pub mod schedule_admin_label;
#[cfg(test)]
#[path = "schedule_admin_label_test.rs"]
mod schedule_admin_label_test;

pub mod schedule_admin_random_merge;
#[cfg(test)]
#[path = "schedule_admin_random_merge_test.rs"]
mod schedule_admin_random_merge_test;

pub mod schedule_admin_scatter_range;
#[cfg(test)]
#[path = "schedule_admin_scatter_range_test.rs"]
mod schedule_admin_scatter_range_test;

pub mod schedule_admin_shuffle_hot_region;
#[cfg(test)]
#[path = "schedule_admin_shuffle_hot_region_test.rs"]
mod schedule_admin_shuffle_hot_region_test;

pub mod schedule_admin_shuffle_leader;
#[cfg(test)]
#[path = "schedule_admin_shuffle_leader_test.rs"]
mod schedule_admin_shuffle_leader_test;

pub mod schedule_admin_shuffle_region;
#[cfg(test)]
#[path = "schedule_admin_shuffle_region_test.rs"]
mod schedule_admin_shuffle_region_test;

pub mod schedule_admin_transfer_witness_leader;
#[cfg(test)]
#[path = "schedule_admin_transfer_witness_leader_test.rs"]
mod schedule_admin_transfer_witness_leader_test;

pub mod schedule_admin_evict_slow_trend;
#[cfg(test)]
#[path = "schedule_admin_evict_slow_trend_test.rs"]
mod schedule_admin_evict_slow_trend_test;

pub mod schedule_admin_balance_range;
#[cfg(test)]
#[path = "schedule_admin_balance_range_test.rs"]
mod schedule_admin_balance_range_test;

mod heartbeat_stream_adapter;

#[cfg(test)]
#[path = "request_role_test.rs"]
mod request_role_test;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod etcd_fixture;

#[cfg(test)]
#[path = "grpc_members_test.rs"]
mod grpc_members_test;

#[cfg(test)]
#[path = "grpc_bootstrap_test.rs"]
mod grpc_bootstrap_test;

#[cfg(test)]
#[path = "grpc_alloc_id_test.rs"]
mod grpc_alloc_id_test;

#[cfg(test)]
#[path = "grpc_store_test.rs"]
mod grpc_store_test;

#[cfg(test)]
#[path = "grpc_store_scenarios_test.rs"]
mod grpc_store_scenarios_test;

#[cfg(test)]
#[path = "grpc_region_lookup_test.rs"]
mod grpc_region_lookup_test;

#[cfg(test)]
#[path = "grpc_region_lookup_scenarios_test.rs"]
mod grpc_region_lookup_scenarios_test;

#[cfg(test)]
#[path = "grpc_region_scan_test.rs"]
mod grpc_region_scan_test;

#[cfg(test)]
#[path = "grpc_region_scan_scenarios_test.rs"]
mod grpc_region_scan_scenarios_test;

#[cfg(test)]
mod grpc_region_query_scenarios_test;
#[cfg(test)]
mod grpc_region_query_test;

#[cfg(test)]
#[path = "grpc_ask_split_scenarios_test.rs"]
mod grpc_ask_split_scenarios_test;
#[cfg(test)]
#[path = "grpc_ask_split_test.rs"]
mod grpc_ask_split_test;

#[cfg(test)]
#[path = "grpc_report_split_test.rs"]
mod grpc_report_split_test;

#[cfg(test)]
mod grpc_cluster_config_test;

#[cfg(test)]
#[path = "grpc_scatter_test.rs"]
mod grpc_scatter_test;

#[cfg(test)]
mod grpc_scatter_scenarios_test;

#[cfg(test)]
mod grpc_split_regions_test;

#[cfg(test)]
pub(crate) mod grpc_split_regions_scenarios_test;

#[cfg(test)]
pub(crate) mod grpc_split_scatter_scenarios_test;
#[cfg(test)]
mod grpc_split_scatter_test;

#[cfg(test)]
mod grpc_global_config_scenarios_test;
#[cfg(test)]
mod grpc_global_config_test;

#[cfg(test)]
mod grpc_global_watch_test;

#[cfg(test)]
mod grpc_global_watch_scenarios_test;

#[cfg(test)]
mod grpc_resolved_ts_test;

#[cfg(test)]
mod grpc_external_ts_test;

#[cfg(test)]
mod grpc_tso_scenarios_test;
#[cfg(test)]
mod grpc_tso_test;

#[cfg(test)]
mod grpc_unsupported_test;

#[cfg(test)]
mod grpc_unsupported_scenarios_test;

#[cfg(test)]
#[path = "http_contract_test.rs"]
mod http_contract_test;

pub mod request_context;
#[cfg(test)]
#[path = "request_context_test.rs"]
mod request_context_test;

#[cfg(test)]
#[path = "http_redirect_test.rs"]
mod http_redirect_test;

pub mod http_redirect;

#[cfg(test)]
#[path = "grpc_forward_test.rs"]
mod grpc_forward_test;

mod grpc_forward;

#[cfg(test)]
mod http_health_test;

#[cfg(test)]
#[path = "http_member_test.rs"]
mod http_member_test;

#[cfg(test)]
mod http_leader_test;

#[cfg(test)]
#[path = "http_cluster_test.rs"]
mod http_cluster_test;

#[cfg(test)]
#[path = "http_cluster_fault_test.rs"]
mod http_cluster_fault_test;

#[cfg(test)]
mod http_config_read_test;

#[cfg(test)]
mod http_config_read_fault_test;

#[cfg(test)]
mod http_schedule_config_write_test;

mod http_schedule_config_write;

#[cfg(test)]
mod http_schedule_config_write_fault_test;

#[cfg(test)]
mod http_replication_config_write_test;

mod http_replication_config_write;

#[cfg(test)]
mod http_replication_config_write_fault_test;

#[cfg(test)]
mod http_server_config_write_test;

mod http_server_config_write;

#[cfg(test)]
mod http_server_config_write_fault_test;

#[cfg(test)]
#[path = "http_store_read_test.rs"]
mod http_store_read_test;

#[cfg(test)]
#[path = "http_store_state_test.rs"]
mod http_store_state_test;

pub mod http_store_state;

#[cfg(test)]
#[path = "http_store_labels_test.rs"]
mod http_store_labels_test;

mod http_store_labels;

#[cfg(test)]
#[path = "http_store_limit_test.rs"]
mod http_store_limit_test;

mod http_store_limit;

#[cfg(test)]
#[path = "http_region_lookup_test.rs"]
mod http_region_lookup_test;

mod http_region_lookup;

#[cfg(test)]
#[path = "http_region_scan_test.rs"]
mod http_region_scan_test;

mod http_region_scan;

#[cfg(test)]
#[path = "http_region_top_test.rs"]
mod http_region_top_test;

mod http_region_top;

#[cfg(test)]
#[path = "http_region_stats_test.rs"]
mod http_region_stats_test;

mod http_region_stats;

#[cfg(test)]
#[path = "http_region_check_test.rs"]
mod http_region_check_test;

mod http_region_check;

#[cfg(test)]
#[path = "http_hotspot_test.rs"]
mod http_hotspot_test;

pub mod http_hotspot;

mod http_hotspot_history;
#[cfg(test)]
mod http_hotspot_history_test;
#[cfg(test)]
mod http_operator_read_test;

pub mod http_operator_read;

#[cfg(test)]
#[path = "http_operator_write_test.rs"]
mod http_operator_write_test;

pub mod http_operator_write;

#[cfg(test)]
mod http_scheduler_test;

pub mod http_scheduler;

#[cfg(test)]
#[path = "http_scheduler_config_test.rs"]
mod http_scheduler_config_test;

pub mod http_scheduler_config;

#[cfg(test)]
#[path = "http_checker_test.rs"]
mod http_checker_test;

pub mod http_checker;

#[cfg(test)]
mod http_region_schedule_test;

#[cfg(test)]
#[path = "http_rule_test.rs"]
mod http_rule_test;

#[cfg(test)]
#[path = "http_rule_fault_test.rs"]
mod http_rule_fault_test;

#[cfg(test)]
#[path = "http_rule_group_test.rs"]
mod http_rule_group_test;

pub mod http_rule_group;

#[cfg(test)]
#[path = "http_region_label_test.rs"]
mod http_region_label_test;

pub mod http_region_label;

#[cfg(test)]
#[path = "http_affinity_read_test.rs"]
mod http_affinity_read_test;

pub mod http_affinity_read;

#[cfg(test)]
#[path = "http_affinity_write_test.rs"]
mod http_affinity_write_test;

pub mod http_affinity_write;

#[cfg(test)]
#[path = "http_admin_cache_test.rs"]
mod http_admin_cache_test;

#[cfg(test)]
#[path = "http_admin_markers_test.rs"]
mod http_admin_markers_test;

#[cfg(test)]
#[path = "http_admin_recover_id_test.rs"]
mod http_admin_recover_id_test;

#[cfg(test)]
mod http_admin_recover_tso_test;

mod http_admin_recover_tso;

#[cfg(test)]
#[path = "http_resolved_ts_test.rs"]
mod http_resolved_ts_test;
