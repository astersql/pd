# Heartbeat effects

`submit_region_heartbeat` accepts a domain report under the host's state mutex,
using the canonical preparation, inheritance, guide and root recheck. Acceptance
returns before asynchronous persistence completes. `HeartbeatRunners` borrows
three already-started canonical `ConcurrentRunner` lanes; the host owns their
runtime, cancellation and stop/join. No worker, runtime or protocol is recreated.

The guide selects subtree repair, overlap classification/label/rule-cache cleanup,
region classification, metadata persistence and follower synchronization. Duplicate
reports only collect when `region_stats_need_update` detects a configuration change
or absent classification, and repair subtrees when root business references are
below two. Cache-changing subtree tasks use guide.retained; duplicate repair and
sync tasks are retained. All queue submissions remain keyed by region and Go task
name, including the distinct ObserveRegionStatsAsync and CollectRegionStatsAsync.

Collect re-reads the latest root and current host configuration/stores/rules at
execution time, skips removed IDs and respects uninitialized placement managers.
The host supplies observation time in Unix seconds. Persistence and history retain
the submitted report. The endpoint saves RegionMeta only; history records full
RegionInfo, with indexes independent of MetaStore revisions.

Submissions follow Go order: root, subtree, overlap cleanup, collection, KV, sync.
Lanes can finish in any order and active same-ID writes may finish out of order.
Within one KV task all overlap deletes are attempted before save; a failed delete
does not stop subsequent deletes/save. Storage failure or runner rejection leaves
accepted root state intact, and a duplicate does not retry a failed metadata write.
An optional Downstreams owner appends and wakes real sync history after admission;
it does not substitute metadata watch or MetaStore revision for sync history.

`HeartbeatEffectsReceipt::wait` reports typed storage/history/admission failures.
Queued replacement, cancellation or panic drops a task receipt and resolves it as
Cancelled. Dropping the receipt does not cancel its task. Shutdown requires the
host to cancel and await every runner; stopped tasks can leave root-only entries,
which later duplicate reports repair. Runners may cancel in-flight I/O, whose
commit outcome can remain unknown. No rollback or automatic write retry is added.

This boundary does not make scheduling decisions or transport conversions. Hosts
still own hot-stat submission, affinity invalidation, scheduling-service placement,
metrics aggregation and the runners' lifecycle. Optional statistics, storage and
Downstreams owners express which effects are locally installed. Rules and anomaly
classification remain real domain components, not mocked calculation callbacks.

Tests are explicitly registered in lib.rs. They check cache/KV/history snapshots,
root-only repair, current-configuration reclassification, latest-root observation,
retained admission under queue aging, stopped/cancelled work, best-effort deletion,
MemoryStore and real EtcdMetaStore split/merge, and real etcd interruption/restart.

The sync effect acquires the canonical HeartbeatEffectsState lock around history
append/broadcast. RegionSyncer attached to this state captures and binds under the
same gate; locks are released before any transport I/O. Lock order is state then
Downstreams, consistent with subscription capture and all other heartbeat effects.
