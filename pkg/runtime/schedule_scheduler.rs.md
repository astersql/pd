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

任务65增加 `configure`：同一coordinator队列独占访问原BuiltinScheduler，保留
method、body、query及局部HTTP响应，穷尽分发18类 `schedule_admin_*`。
最后store删除的同步回调先恢复领域内存，再在相同独占区间执行异步Controller.remove；
失败保留原策略，成功清理配置。`observe_hot_support` 只在实际调度尝试时更新
版本能力观察，配置GET不触发检查。真实网络、事务故障与Go差分见
[任务65](../../docs/rust/server-api/task-065.md)。
