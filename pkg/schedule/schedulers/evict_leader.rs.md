# Leader eviction domain boundary

`EvictLeaderConfig` owns an immutable, cloned snapshot of store IDs and key ranges,
with batch validated in 1..=100 (default 3). Empty store ranges disable that store;
an empty start/end pair covers the keyspace. JSON uses the Go `store-id-ranges`
and `batch` names and the existing binary-key codec. BTreeMap deduplicates store
IDs and gives deterministic iteration; no extra state store or runtime is owned.

`EvictLeaderScheduler::prepare/clean` pause/resume only transfer-in using the
canonical store registry. Prepare attempts every store, retains partial success
and returns the last StoreControlError, following Go. The coordinator must pair
one clean with each prepare attempt, including failures; these methods are not
idempotent because the shared registry uses directional reference counts.
Transfer-out pauses do not block administrative eviction: Go applies only the
target store-state filter here. Independent pause references remain intact.

`schedule` borrows canonical BasicCluster, SchedulerConfigProvider and RuleManager
through EvictLeaderContext. Clock, random index and range shuffle are host inputs.
It samples configured leaders fully contained in ranges, preferring regions with
no down/pending peers. If none is sampled, a second sample permits unhealthy
regions and explicitly excludes their down/pending peer stores. Urgent target
filtering and PeerOperatorFactory create real leader operations with all surviving
targets. Factory errors remain available as typed BuilderError diagnostics; one
factory failure does not fabricate an operation. No peer IDs are allocated.

Each round attempts one region per store. Duplicate region IDs are suppressed
across the whole batch. Go's strict `len(ops) > batch` check and whole-round append
are retained: two stores, batch 3 can yield 4 operations. Stable sorting of store
and candidate IDs only fixes unspecified map ordering; chosen primary targets
remain controlled by the supplied random index. No business error is normalized.

This is source/test-derived migration evidence under the user's plan-wide
explicit authorization; there is no Go execution oracle or runtime differential
claim. Read-only sources are evict_leader.go, its adjacent tests, existing core
sampling/filter/operation factories. See
[task-075 evidence](../../../docs/rust/schedule-execution/task-075.md) and its
fixture/logs for provenance, red/green tests and real operation completion.
HTTP, configuration transactions and persistent recovery belong to adapters and
subsequent task 76. This task adds no dependency or Cargo feature.
