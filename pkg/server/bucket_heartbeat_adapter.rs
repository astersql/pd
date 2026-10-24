// Copyright 2026 AsterSQL.
//! Domain-only host boundary for the client-streaming bucket report route.
use astersql_pd_cluster::BucketRegionNotFound;
use astersql_pd_statistics_buckets::{BucketFlows, BucketReport};

/// Host owns canonical region admission and optional local hot-bucket worker.
/// Use `handle_bucket_heartbeat` under the host cluster lock: only applied
/// reports enter that worker, including equal/stale versions accepted by core.
#[async_trait::async_trait]
pub trait BucketHeartbeatService: Send + Sync {
    fn is_bootstrapped(&self) -> bool {
        true
    }
    async fn bucket_heartbeat(&self, report: BucketReport) -> Result<bool, BucketRegionNotFound>;
}

/// Moves raw counters and ordered keys without normalizing units or versions.
/// Array-length requirements are the canonical bucket worker's input contract.
pub fn bucket_from_proto(buckets: astersql_pd_proto::metapb::Buckets) -> BucketReport {
    let stats = buckets.stats.unwrap_or_default();
    BucketReport {
        region_id: buckets.region_id,
        version: buckets.version,
        keys: buckets.keys,
        period_in_ms: buckets.period_in_ms,
        flows: BucketFlows {
            read_bytes: stats.read_bytes,
            read_keys: stats.read_keys,
            read_qps: stats.read_qps,
            write_bytes: stats.write_bytes,
            write_keys: stats.write_keys,
            write_qps: stats.write_qps,
        },
    }
}
