# Server public surface

PdServices supplies identity, health, ID and TSO operations. The default
store_heartbeat_service accessor returns None, retaining opt-in host ownership for
the state service. Hosts may implement the domain-only StoreHeartbeatService and
use apply_store_heartbeat with their existing canonical cluster and statistics.
router composes HTTP and gRPC under one host cancellation scope. Production and
separate test modules, including store_heartbeat_adapter_test, are explicitly registered.

The optional region_heartbeat_service accessor similarly defaults to None;
RegionHeartbeatService supplies domain-only membership and region admission.
The region adapter and its separate behavior tests are explicitly registered.

The optional bucket_heartbeat_service defaults to None. BucketHeartbeatService
and its separate test module are explicitly registered; the host supplies the
canonical cluster admission and local worker without adding protocol types to domain crates.
