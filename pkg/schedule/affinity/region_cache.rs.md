# Region cache and external synchronization

`Manager::get_region_affinity_group_state` is a read-only query;
`get_and_cache_region_affinity_group_state` additionally records successful
ownership. Inputs use optional `Arc<RegionInfo>` snapshots so pointer identity
matches Go's immutable region pointer contract. Time is explicit Unix seconds;
label queries convert it to nanoseconds. Core voters and labeler matching remain
canonical. Learners do not participate in affinity. Missing ownership deliberately
retains a previous entry, as in Go; callers notify deletion/merge via `invalid_cache`.

Entries contain the snapshot, group ID, version, and affinity decision. Group
version changes invalidate affinity without dropping membership. Removing stale
entries decrements membership but never decrements a newer affinity count.
`affinity_region_count` sums current group counts instead of storing a second
mutable total. `clear_group_cache` is also used after successful local range
removals and group deletions. Failed local storage operations never reach cleanup.

`sync_group_from_etcd` initializes degraded groups, attaches an existing labeler
rule or a buffered label-first event, and treats identical peer layouts as no-ops.
Existing timestamps and availability are preserved. Changed layouts reset affinity
and bump the version. `sync_key_ranges_from_etcd` validates extraction before
mutation, buffers only nonempty rules for absent groups, and preserves membership
while resetting affinity for existing groups. Empty updates clear range metadata;
`sync_key_ranges_delete_from_etcd` also clears region membership. Group deletion
clears regions, ranges and buffered rules. Prefix checks reject unrelated deletion
IDs. Events follow arrival order; revision ordering belongs to the host watcher.

These synchronous entry points do not write storage or modify RegionLabeler. The
host delivers labeler updates independently and supplies exclusive manager access.
There are no background tasks, private runtimes, transport types, or new dependencies.
Tests are explicitly registered from `affinity_cache_sync_test.rs`; real MemoryStore
and EtcdMetaStore share the main contract. See [batch 38 evidence](../../../docs/rust/schedule/affinity/38-证据.md).
