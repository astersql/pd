# Region point lookup adapter

`Grpc` serves GetRegion, GetPrevRegion and GetRegionByID through the same router
and host-owned canonical cache used by heartbeats. Admission precedes cluster ID
validation. A follower requires the presence of `pd-allow-follower-handle` metadata
(the value is irrelevant) and reads only the synchronized follower projection.
Cold/missing follower results have REGION_NOT_FOUND; leader misses have an empty
successful header. Bootstrap and server cancellation preserve their existing
protocol ordering. The outer cancellation select covers admission and pending
cache reads.

`RegionsInfo::region_by_key` holds one canonical index read lock and returns the
immutable full snapshot. Transport conversion retains region, leader, down and
pending peers. Requested buckets are gated by host store configuration, default
false, and never returned by followers. The adapter owns no domain indexes,
listeners, runtimes or workers.

`grpc_region_lookup_test.rs` exercises real etcd, two runtime nodes and TCP RPCs.
Shared scenarios also run against the existing Go binary through the read-only
supervisor. `pkg/runtime/grpc_region_lookup_fault_test.rs` exercises configured
buckets and cancellation behind a real pending metadata transaction. See
`docs/rust/server-api/task-023.md` for current commands and evidence.
