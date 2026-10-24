# gRPC adapter

Transport boundary for the minimal PD role. Existing GetMembers, AllocID and TSO
behavior remains unchanged. StoreHeartbeat optionally calls the host's explicit
StoreHeartbeatService with losslessly converted domain statistics. The route checks
cluster ID and health, preserves transport versus response-header errors and maps
NodeState values without importing protobuf into the cluster library.
See store_heartbeat_adapter.rs.md for field presence, unsupported controller inputs,
state ownership and lifecycle boundaries. No worker or runtime is created here.

RegionHeartbeat uses the optional domain-only region state service. The response
stream owns all receive/acceptance work, sends errors without success acknowledgements,
and drops the in-flight future on cancellation or disconnect. See
region_heartbeat_adapter.rs.md for validation order and excluded scheduling work.

ReportBuckets consumes a client stream through the optional BucketHeartbeatService.
It preserves nil/empty filtering, silent domain errors, no per-report ack and Go's
client-observed Unknown/EOF closure. The RPC owns in-flight work and cancellation;
see bucket_heartbeat_adapter.rs.md for units, applied admission and host ownership.
