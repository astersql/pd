# Coordinator background lifecycle

`Coordinator::run` waits for the existing sticky preparation gate (3-second
checks), restores configured schedulers with task 114, then returns
`CoordinatorRun`. Its `wait` future drives concurrent workers on the caller's
executor. `run_until_stop` combines these two operations. The host must poll the
future alongside heartbeat routing, cancel the token/handle, and await it before
shutting down host caches, storage, or streams. Dropping a future is not graceful
shutdown. No runtime, thread, global subscriber or process exit is installed.

The four coordinator workers reuse `Patrol::run`, `SuspectRanges::run`, canonical
`ControllerDispatch`, and the built-in slow-trend factory/controller. Suspect
ranges borrow the same checker and BasicCluster; no heartbeat state is copied.
The caller supplies a `PatrolHost` over the coordinator's checker, cluster and
`operator_handle` (the same Arc-backed admission controller), as required by the
existing patrol API. The host continues to own rules, labeler and statistics.

The host supplies its existing `ScheduleController::run` futures through
`scheduler_workers`; all receive the coordinator child cancellation token. This
preserves the existing separation between scheduler management and concrete
policy execution, without adding a new policy adapter or runtime. The lifecycle
joins every supplied future, including its policy cleanup, before returning.
Callers must register all their scheduler workers and must not detach work from
these futures. Dynamic policies remain on the existing host-owned scheduler
execution path; the minute worker creates/prepares/persists the slow-trend entry
in the canonical management controller.

Push checks run every 500ms, register newly admitted operator identities with the
existing deadline queue, dispatch due commands, record terminal operations and
success callbacks, record non-witness TTL, and promote waiting batches when
requested. The clock, random draw, negotiated protocol version and cluster ID
are explicit host inputs. Slow-node checks run every minute, exit if the entry
exists (including handlers), and only create it for raft-kv2. Add failures are
nonfatal and retry on the next minute; errors remain in controller diagnostics.

The supervisor cancels siblings after a typed patrol, routing, admission or
scheduler error and drains all workers, allowing in-flight work to finish.
`clean_registered` is the necessary management API extension: release prepared
store controls after the join without disabling/deleting startup configurations.
Startup restore failures clean partially prepared entries and start no workers.
Stop is idempotent. A coordinator instance has one startup/shutdown lifecycle;
restart uses a fresh facade and the persisted configuration.

Readonly Go `coordinator.go` Run/RunUntilStop/Stop, push and slow-node loops define
these contracts. Go schedules goroutines immediately; the Rust borrowed API is
explicitly host-driven and begins polling workers when `wait` is polled. Go
runtime differential evidence is not claimed: the user authorized source
inference with real Rust behavior/fault validation. See
[task 115 evidence](../../docs/rust/schedule-execution/task-115.md).
