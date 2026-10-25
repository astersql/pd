# Merge decision and split cooldown

`MergeChecker::new` records startup time and the configured signed nanosecond
interval. `record_region_split` records IDs at the host's Unix nanosecond timestamp;
`check` consumes canonical region, store, placement, label and hot-ID snapshots.
The host serializes this mutable checker and supplies time; it creates no runtime,
background task, storage backend or duplicate statistics state.

The source contract is `merge_checker.go` Check/RecordRegionSplit/checkTarget,
`region.go` NeedMerge, `ttl.go` get/UpdateTTL, and the adjacent Go checker tests.
Under the user's oracle waiver, expectations are source derived, **not executed
Go differential results**. See [task evidence](../../../docs/rust/schedule-execution/task-047.md).

Pause precedes TTL updates. Interval changes reset retained split deadlines before
startup gating. Startup opens at equality; split TTL expires strictly after its
deadline. Expired entries are collected synchronously during unpaused checks;
this replaces Go's asynchronous GC, whose incidental scheduling is not modeled.
Configuration resets apply before this collection, preserving the observable
UpdateTTL behavior of retained entries. The host should evaluate periodically to
bound retained cache memory. Repeated split records replace deadlines.

Source health, replication, leadership, heat and inclusive size/key thresholds
precede target selection. The next valid neighbor wins ties; bidirectional mode
selects a strictly smaller previous neighbor. A selected target failing the final
size/key checks does not cause fallback to a larger target. Target cap is the
larger of 500 MiB and four times store maximum size; canonical store checks reject
merge/split loops. Zero thresholds still admit an actually empty region, matching
NeedMerge; scheduling limits belong to the controller.

The canonical `RangeOperatorFactory` creates active/passive paired operators,
including real peer alignment and ID allocation where needed. Gate rejection
returns `Ok(None)`; construction errors propagate as `BuilderError`, retaining
an explicit diagnostic in place of Go's global warning logger. Production check
performs no persistence I/O. Tests initialize real RuleManager through existing
MemoryStore/StorageEndpoint and inject allocation failure at the factory boundary.
No partial pair is returned on failure. Modules remain in the canonical checker
crate; no new dependency or feature was introduced.

The separate `task_047_test.rs` covers 43 fixed scenarios and 23 sequential
cooldown assertions, plus paired IDs/epochs/kinds/priorities, owned merge metadata,
commands, merge confver and influence. Fault-mutation logs prove tests detect
incorrect startup, split source/target gating, TTL boundary/update, direction,
selection, health and target cap. See task evidence for exact commands and counts.
