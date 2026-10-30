# Serialized scheduler lifecycle

`ClusterHost::scheduler` sends a command to the existing term-scoped coordinator
queue. `schedule_scheduler.rs` validates leadership and exclusively borrows that
coordinator's SchedulerController. The ordinary worker ticks those same policies,
consulting their pauses, and sends real operators through the heartbeat router.

Canonical registry factories create policies. The controller prepares, saves,
removes and cleans policies. Runtime publishes the controller's scheduler list to
its existing persisted and live configuration owners; it preserves Go's retained
in-memory option changes when the final configuration transaction fails.

Repeated evict/grant creation updates the retained policy; repeated balance-range
creation adds a canonical job. Store-suffixed deletion returns the redirected
route's status with a null body. Pause timestamps are term memory and reset on
restart, while scheduler creation and removal use the existing fenced MetaStore.

See [task 064](../../docs/rust/server-api/task-064.md). No transport or etcd types
were introduced into the scheduling domain.
