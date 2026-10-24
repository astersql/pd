# Manager startup and restore

`Manager::initialize(&StorageEndpoint, &ClusterPaths, &RegionLabeler, now_seconds)`
loads raw affinity records, ignores malformed JSON, uses logical group IDs rather
than physical suffixes, and retains the first duplicate ID. `null` restores a zero
value, matching Go encoding/json. Restored groups start degraded with a 600-second
expiration, version 1 and empty region counters.

It walks the labeler's authoritative rules, skips foreign identities, unknown
groups and extraction errors, caches nonempty ranges, and attaches valid rules
(including empty data). Attachment increments the version and sets the adjusted
range count. The shared range validator rejects overlap before the snapshot is
returned. Storage errors propagate through `RestoreError::Storage`; overlap uses
`RestoreError::Ranges` and the existing typed PD error.

`Manager` owns `groups` and `key_ranges`. `RestoredGroup` contains canonical
`RuntimeGroupInfo`, `affinity_version` and `label_rule`. Initialization performs no
writes and starts no tasks. Host orchestration and later tasks own policy loops,
locking, transactions, region caching and synchronization. No protobuf types enter
this API. Storage endpoint retains its existing protocol-adapter dependencies.

Tests are registered in `lib.rs` from `affinity_restore_test.rs`. They use real
memory/etcd endpoints, the same etcd WAL across a process restart, and explicit
seconds for expiration boundaries. See [batch 33 evidence](../../../docs/rust/schedule/affinity/33-证据.md).

Batch 36 adds a private unavailable-store snapshot and synchronous health policy;
see [policy module](policy.rs.md). The host still owns loop lifecycle.

Batch 38 adds owned region snapshots and a label-first synchronization buffer;
see [region cache and synchronization](region_cache.rs.md). Hosts supply exclusive
access to synchronous mutations and deliver labeler events separately.
