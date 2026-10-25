# Priority inspector

`PriorityInspector::inspect` reads canonical `RegionInfo`, config, stores and
`RuleManager::fit_region`. Replica mode counts all peers. Placement mode sums
`rule.count - rule_fit.peers.len()` for non-learner rules and returns the actual
`Arc<RegionFit>` for downstream reuse. An empty rule fit has no deficit. Role
mismatches inside assigned peers are counted; repairing their role belongs to
other checkers. No operator or storage transaction is created by inspection.

The inspector owns a mutex-protected canonical `astersql-pd-cache::PriorityQueue`
with default capacity 1280. Lower numeric priority means a larger replica deficit.
Queue equivalence is priority-only, as in Go: equal priorities replace the sorted
entry while both IDs can remain in the lookup map. Removing a displaced tied ID
also removes its priority's current sorted entry. This is deliberate compatibility,
not a guarantee that every deficient region is simultaneously enumerated.

Repeated equal-priority inspection increments attempt and replaces last time.
Changing an existing ID's priority retains its original retry state because
canonical Put preserves the original value. Epoch is not part of queue identity.
Recovery or excess replicas removes the ID. Full queues admit only a new ID that
strictly outranks their worst priority; rejected IDs have no partial entry.

The host supplies nanoseconds and config snapshots; no runtime or timer is
created. `get_priority_regions` is read-only and eligible strictly after
`last + attempt * 10 * patrol interval`. Retry arithmetic saturates at Rust
integer boundaries, avoiding panics on pathological host clocks/counters. Mutex
poisoning follows the canonical cache's panic policy. There is no fallible I/O
inside this inspector; placement persistence remains owned by RuleManager.

Source basis: read-only `priority_inspector.go`, `priority_inspector_test.go`,
`cache/priority_queue.go`, and `placement/fit.go`. Per user authorization there is
no Go runtime differential claim. [Task evidence](../../../docs/rust/schedule-execution/task-051.md)
records fixed fixtures, negative behavior experiments, real placement fitting,
concurrent removal, Cargo slot ownership and workspace verification.
