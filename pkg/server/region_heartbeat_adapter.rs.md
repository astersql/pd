# Region heartbeat state boundary

RegionHeartbeatService is explicitly supplied by PdServices; the default accessor
returns None. The host owns bootstrap/membership state, flow divisor, cluster locks,
clock and region acceptance/side effects. It calls canonical cluster admission and
may attach its existing effects pipeline; no parallel index or statistics algorithm
is implemented here. Domain operations receive RegionInfo without protobuf/tonic.

The gRPC stream uses core_region_adapter::region_from_heartbeat to preserve epoch,
leader, term, interval, query and CPU fields, including absent messages. Size enters
in bytes and core normalizes once; query excludes GC, CPU uses unified_read and
ignores scheduler. The host supplies current store membership: missing store is a
terminal Unknown status, Removed skips the report. Bootstrap failure produces a
terminal NOT_BOOTSTRAPPED header. Invalid region and state failures produce UNKNOWN
headers targeted at the report leader; region_id/epoch retain SendErr defaults.
Go protobuf dump suffixes are intentionally omitted from validation diagnostics.

Accepted/duplicate reports send no success acknowledgement. The output stream owns
the receive loop and in-flight host future; EOF, disconnect and cancellation drop
that future with no detached worker. The host must arrange cancellation-safe effects
and must not detach work through this contract. No runtime, listener or persistent
backend is owned here. Scheduling commands, hbstream binding across connections,
keepalive, forwarding and controller decisions are outside this state boundary.

Tests use real BasicCluster admission and tonic HTTP/2 loopback; see
region_heartbeat_adapter_test.rs and docs/rust/state/task-48.md for Go evidence,
negative cases, field snapshots and cancellation/drop verification.
