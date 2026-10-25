# Heartbeat dispatch and stale operations

`ControllerDispatch` borrows the canonical `ControllerAdmission`, `BasicCluster`,
configuration provider and heartbeat `RoutingHandle`. It owns only its retry heap
and fast-success influence cache. It installs no runtime, worker, signals,
subscriber or storage backend; protobuf remains in transport adapters.

`dispatch` calls the real `Operator::check` before examining status. STARTED
heartbeat operations validate the current step, then compare wrapping unsigned
confver changes with `Operator::conf_ver_changed`. Only TransferLeader receives
the operator's opt-in unhealthy permission; steps still perform their existing
unconditional Down checks. Create and notifier sources bypass stale checks,
matching Go. Successful completion wins over epoch rejection and timeout.

All twelve OpStep variants delegate command construction and validation to their
canonical implementations. Commands snapshot cluster ID, observed region ID,
epoch and leader through `HeartbeatCommand::for_region`. Absent leaders and
suppressed step commands enqueue nothing. The adapter supplies negotiated V2
support. Queue closure returns `RoutingClosed`; transport failure unbinds the
stream in the existing router, preserving the running operation for retry/rebind.
Queue admission is distinct from delivery.

`poll_due` resolves the current operation by region ID even for an old heap entry,
checks missing regions/newer related-merge range versions before deadlines, and
advances completed steps before checking the deadline. Fast steps retry in two
seconds, other steps in five. A future deadline is reinserted and stops polling;
a missing entry is discarded and polling continues. `push_due` drains that
host-driven loop using explicit monotonic and Unix clocks.

Terminal removals use identity checks and the existing store-budget Ack path,
including reciprocal merge removal. Success detaches without inventing a cancel
reason. Successes shorter than ten seconds enter the existing influence cache.
`DispatchOutcome` exposes detached operations, completion and nonwitness TTL
recording effects. The host handles history/callbacks and calls the already
implemented `promote_limited_waiting` when `promote_waiting` is true, passing its
random sample; it schedules new admissions and dispatches Create immediately.
This follows the existing admission layer's host composition boundary. For
immediate callback processing or partial progress on queue errors, hosts should
consume `poll_due`/`dispatch` one event at a time; `push_due` returns an error if
any enqueue fails. Admission/removal may race as in Go; failed identity removal
cannot remove a replacement.

Evidence is **source-derived**, not a Go execution comparison, under the user's
explicit oracle admission waiver. Source methods: Dispatch, checkStaleOperator,
SendScheduleCommand, getNextPushOperatorTime, pollNeedDispatchRegion and
checkOperatorLightly in `operator_controller.go`. Read-only adjacent contracts:
TestDispatchOutdatedRegion, TestDispatchUnfinishedStep, TestPollDispatchRegion,
TestPollDispatchRegionForMergeRegion. Existing concurrency/limiter tests remain
in the crate suite. See [task-036](../../../docs/rust/schedule-execution/task-036.md)
for frozen provenance, mutation failure, behavior coverage and validation logs.
