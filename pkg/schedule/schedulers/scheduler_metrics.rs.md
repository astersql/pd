# Scheduler metrics

`scheduler_metrics.rs` is the canonical host-owned collector implementation for
`pkg/schedule/schedulers`. It registers all 20 families from Go `metrics.go` in the
caller's `prometheus::Registry`, preserving name, help, type, label names and the
30 exponential hot-peer histogram buckets. Clones share collectors; different
registries are isolated. The 101 unique eager zero-valued event label pairs from
Go initialization are present before the first scheduling attempt. Duplicate registration returns `prometheus::Error` and
rolls back this call's earlier collectors without unregistering host collectors.
There is no global registry, background task, private runtime or protocol type.

`event_metrics` returns a cached `Counter` for explicit source event labels.
`scheduler_event_metrics` applies policy label conventions for all 18 built-ins,
including Go's grant-leader → grant-hot-region alias and instance names for
balance-witness/slow-trend. `MetricsScheduler<S>` implements the existing
`LoopScheduler` contract, forwarding actual policy execution and cleanup while
incrementing the cached schedule counter for each entered attempt. Cancellation
before execution creates no event. Empty retries and policy errors remain real
attempts. Cleanup and dropping the wrapper delete only its `name/allow` status.
Operator admission and individual policy reason events use cached handles at
those host boundaries; the wrapper does not count an unadmitted operator as an
admission event. It does not automatically wrap existing host-created policies.

`record_metrics` is the explicit adapter for store gauges, direction/result
counters and hot-peer observations. It checks family, label arity and finite
values, rejecting negative counter increments before adding series. Store IDs
appear only in the source's per-store labels. Dynamic label sets are retained
for partial deletion under a shared mutex; add/delete/reset cannot corrupt the
index. Cached event handles remain valid because Go's reset preserves events.
Do not cache handles for resettable/deletable store families across cleanup.

`collect_scheduler_metrics` takes real RuleManager and RegionLabeler references;
status depends on pause/halt, independently of policy scheduling limits. Missing
managers leave existing manager series untouched, as Go does. Removal requires
`remove_scheduler_metrics`; collection is not implicit garbage collection.
`reset_scheduler_metrics` resets precisely Go's 13 families. It preserves events,
tolerant resource, hot peer histogram, hot pending, trend action/misc and range job.
`delete_store_metrics` deletes precisely 10 partial store families plus the two
`move-witness/store-in|out` counter series; exact string matches preserve store 10
when deleting store 1. Repeated deletion is harmless and live entities may publish
again after cleanup, matching Go's metric vector lifecycle.

`collect_hot_pending_metrics` publishes signed `StoreSummaryInfo.pending_sum`
from the canonical hot projection output in all 8 RegionStatKind dimensions.
It checks fresh cluster liveness before and after publication, not stale summary
metadata. The second check deletes only this store's HotPendingSum. As in Go,
a removal after that check can leave a bounded residual until the host's next
cleanup; no stronger cross-component atomicity is claimed. Empty/missing sums
and missing/removed stores do not create series. Short or nonfinite input returns
a typed Prometheus error rather than Go's short-slice panic/NaN publication;
valid finite loads preserve source behavior. A batch is not transactional across
stores: prior valid stores remain published if a later store is invalid.

Oracle provenance is **read-only Go source derivation**, under explicit user
waiver; this task has no Go runtime oracle or runtime differential claim.
Sources include `metrics.go`, controller Collect/Reset/Remove, hot-region pending
publication and its bury-race test. Fixed definitions and cleanup membership are
in [task_110.json](../../../docs/rust/schedule-execution/fixtures/task_110.json).
[Task evidence](../../../docs/rust/schedule-execution/task-110.md) records actual
failure, mutation, success, registry fault, canonical component and baseline tests.

任务116补充`delete_hot_pending_metrics(Some(store))`与`None`：供canonical
schedule门面执行按store清理或全pending reset，同步维护vector及series集合，
不重置其它scheduler观测。验证见[任务116](../../../docs/rust/schedule-execution/task-116.md)。
