# Disk slow-store eviction

`EvictDiskConfig` / `EvictDiskScheduler` implement the disk branch of Go
`evict_slow_store.go`. This is the canonical disk implementation in the scheduler
crate. It borrows the host's BasicCluster, SchedulerConfigProvider, RuleManager
and StorageEndpoint; it creates no runtime or second state registry.

`observe` selects a unique preparing/serving slow store: all stores matching
`is_slow` (score >=80 or slow-trend eviction) count toward ambiguity, even if
only one reaches the eviction threshold of 100. Removed/removing stores do not
become candidates. For an existing eviction, score <=1 starts recovery; higher
scores reset recovery status. The timestamp changes only on status transitions.
The explicit host clock replaces time.Now/time.Since; it must be nondecreasing.
Elapsed duration saturates at Go's signed time.Duration maximum, including the
zero-time case. The default recovery duration is 1800 seconds; batch is 3.

`prepare` restores the first persisted eviction reference. `clean` persists the
empty eviction set and releases that reference. As Go does, failed clearing
retains the old configuration but still releases the cluster reference; retrying
Clean can therefore decrement it again. `is_recovered` deliberately survives a
clear and recapture. It and the timestamp are process-local and disappear on
JSON reconstruction. JSON emits disk fields `recovery-duration`, `evict-stores`
and `batch`; missing/null store lists become empty, missing/zero batch becomes 3.
The domain constructor and decode boundary reject batches outside 1..100.

`persist` publishes a new snapshot only after real MetaStore I/O succeeds.
Acquisition rolls back on failure or cancellation while waiting for I/O. Errors
retain typed StoreControlError / SchedulerError sources. Existing evictions and
recovery transition snapshots remain intact on failed saves.

`schedule` reports state errors separately while continuing eviction over the
retained configuration, matching Go's log-and-continue schedule path. It reuses
EvictLeaderScheduler's canonical batch, filters and PeerOperatorFactory through
an internal description parameter. The description is evict-slow-store-scheduler;
kind LEADER, priority URGENT, full keyspace, candidate set, confver influence and
heartbeat transfer commands come from real operators. Duplicate region IDs are
removed. Random choices are host-owned; tests use fixed draws and stable target
ordering, checking all candidate targets rather than Go map iteration order.

Network detection, network average-score helpers, HTTP routing/config mutation
and metrics are outside this disk task. This config is the disk-only subset and
must not be used to rewrite a combined network configuration. A later network
policy can extend the canonical slow-store configuration; no network behavior
or full scheduler registration is claimed here. Public module exports follow
the existing concrete scheduler registration convention.

Go source/tests are read-only design oracles under the user's plan-wide
exception. There is no Go execution or runtime differential claim. The suite,
TestEvictSlowStoreBatch and TestRecoveryTime inform disk cases; HTTP batch-limit
tests inform typed validation. TestCalculateAvgScore covers a network helper,
so it is explicitly outside this implementation. Source/fixture hashes,
commands, red/green evidence and counts are in
[task-079](../../../docs/rust/schedule-execution/task-079.md).

任务141新增 `update_policy` 供 HTTP 事务成功后应用数值策略；不重置 evicted、
last_capture 或 is_recovered。完整 envelope 由 network 的 update_with_disk 保存。
