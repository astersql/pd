# coordinator.rs

`Coordinator::new(CoordinatorInputs)` owns one admission controller, terminal records,
checker dispatcher, scheduler controller, prepare checker, diagnostic recorder map and
scatter distribution counters. It retains the supplied `Arc<BasicCluster>` and config.
There is no copied heartbeat collection, private runtime, background patrol or global
registry. Patrol is a subsequent task. Startup configuration restoration is described in
[scheduler_restore.rs.md](scheduler_restore.rs.md).

Checker success registration uses the existing weak dispatcher callback. Terminal
records publish successful operator identities once; the callback reads the current
config limit and host clock. Canceled/replaced operators do not populate affinity TTL.
Publish terminal records after releasing checker locks to avoid reentry deadlock.
`get_rule_checker` and `get_merge_checker` return locked views into that dispatcher;
drop these guards before another facade call that takes the checker lock.

`record_op_step_with_ttl` records actual completed non-witness promotion in the same
rule checker, using current config interval and explicit Unix nanoseconds. The query
used by witness repair is strict at expiration. `is_pending_region` delegates to the
rule pending cache; this is distinct from the pending-processed TTL queue.

Scatter and splitter getters return short-lived views over canonical algorithms,
cluster and admission state, rather than constructing independent controllers.
Scatter counters persist in the coordinator; caller supplies current placement/hot/
affinity inputs. Replication failures from both paths call the same checker's
`add_pending_processed_regions(false, ...)`. Split observation/retry and transport
cadence remain explicit host-driven operations. Returned admissions require the
host's normal dispatch/record path; acceptance alone is not split completion.

The diagnostic getter creates a borrowed `Manager` over this scheduler controller,
config and recorder map. Scheduler and recorder mutations are immediately visible.
The host serializes facade mutation and populates diagnostics through the exposed
canonical recorder map. The supplied heartbeat routing handle remains host-owned;
this task neither starts nor replaces transport routing.

Source-derived contracts: read-only Go `coordinator.go` constructor/getters,
`RecordOpStepWithTTL`, `ShouldRun`, scatter/splitter replication callbacks and checker
success routing. Per explicit user authorization, tests use source derivation and
real Rust state/fault behavior; no Go runtime oracle or differential parity claimed.
See [task 113 evidence](../../docs/rust/schedule-execution/task-113.md).

`init_schedulers` delegates to canonical factory/controller/MetaStore recovery and
marks `are_schedulers_initialized` only after final list cleanup. Cancellation and
exhausted load retries return errors without setting the flag; final persistence
failure is reported but still marks initialization complete, matching Go. The host
supplies the complete persisted JSON to `scheduler_configuration`; the resulting
controller configuration and report's valid list are the authoritative startup
publication for updating a host-owned config provider. No interior mutation of an
arbitrary immutable `Arc<C>` is attempted.

Task 115 shares admission through `operator_handle` (an Arc to this same owner)
so host patrol/scheduler workers can retain it while the lifecycle mutably owns
scheduler management. See `coordinator_lifecycle.rs.md` for cancellation and join.
