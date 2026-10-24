# Store heartbeat state adapter

`StoreHeartbeatService` is an explicit opt-in host interface for owned core StoreStats;
its result uses NodeState and typed state errors, with no protobuf/tonic domain dependency.
The host owns synchronization, Unix-nanosecond clock, BasicCluster and HotStat lifecycle.
`apply_store_heartbeat` checks canonical membership, maps Removed to the protocol-facing
error, and delegates active reports to astersql-pd-cluster::handle_store_heartbeat.
It never persists a stale store snapshot or reimplements statistics, membership or scheduling.
The host must serialize state access and cancel/join any HotStat it supplies.

The gRPC route validates cluster identity and health, rejects absent stats with Unknown,
uses the existing lossless core_store_adapter, and translates typed state errors into
ResponseHeader: UNKNOWN, STORE_TOMBSTONE, NOT_BOOTSTRAPPED. Missing host injection
returns Unimplemented. Unknown NodeState integers pass through. Missing optional stats
messages/interval/query/CPU and repeated peer order are preserved without unit conversion.
Go's missing-stats payload dump is deliberately omitted; the error code and command prefix
remain stable. Recovery and DR-status reports return Unimplemented before state dispatch,
because their controllers and the Go recovery-report bypass are outside this adapter.
ReplicationStatus, ClusterVersion, scheduling/limiter and unsafe recovery output are outside
this state-only interface; responses leave those fields at protobuf defaults.

Tests are explicitly registered in lib.rs. They exercise encoded/decoded messages, actual
HTTP/2 tonic routing, real BasicCluster and HotStat, two region peers with query/CPU loads,
missing stats, unknown/removed stores, optional injection, defaults and response errors.
See docs/rust/state/task-47.md for the real Go oracle and red/green evidence.
