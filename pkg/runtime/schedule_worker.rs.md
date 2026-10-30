# Term-scoped scheduling coordinator

The cluster host starts one worker per serving leader term. It constructs the
existing `Coordinator` over shared canonical cluster handles, the same fenced
MetaStore endpoint, restored dynamic configuration and heartbeat routing handle.
The existing sticky preparation gate admits recovery; `init_schedulers(true)`
prepares the actual registered instances. The cleaned scheduler list is published
back into the host configuration without dropping StoreConfig. The final global
configuration write remains nonfatal and observable through `schedule_error`.

Typed policy futures are local. A task on the supplied Tokio executor's blocking
pool polls a LocalSet through that executor's Handle. The library creates neither
a runtime nor a thread and installs no process globals. Admission via the host
resources lock serializes policy/rule access and heartbeat publication; index
locks never cross awaits. The host joins this task before releasing statistics,
placement state or storage. Cancellation before preparation starts no policies.

Timers use Tokio's host clock. Registered instances retain per-policy intervals;
canonical ScheduleController provides retries, interval growth and label gating.
Actual admissions use the Coordinator's operator controller, including reciprocal
merge batches. Initial sends use ControllerDispatch; heartbeat observations apply
its stale-step/epoch checks. A 500 ms push tick reuses the canonical deadline queue
and fast-operation cache. The minute raft-kv2 check adds the canonical slow-trend
policy. The worker retains restoration/execution errors for host inspection.

Streams are term-bound and routed against shared store membership. Transport
sends have a deadline and cancellation; the router joins in-flight I/O during
shutdown. Cleanup uses `clean_registered` after scheduling stops, leaving durable
startup entries intact. This worker does not claim the separate patrol/checker
integration of the following task is implemented.

Tests and real Go comparison: `schedule_worker_test.rs` and
`docs/rust/server-api/task-017.md`. The TCP scenario restores evict-leader,
verifies transfer-leader delivery/rebind, rejects stale input, checks durable
region/scheduler state and joins shutdown. Default policies execute over cold
real statistics; their complete hot/load behavior is covered by existing domain
tests, not claimed as new full-policy Go differential coverage here.


`ClusterHost::watch_schedule` publishes Starting/Preparing/Running/Stopped and
completed tick progress without retaining workers or storage. Controlled-clock
hosts can wait for a real lifecycle boundary before advancing time; a fixed
number of yield_now calls cannot synchronize a blocking-pool actor. Runtime tests
use this progress only to coordinate execution and assert real commands/state,
never as a substitute for behavior evidence.

任务66复用当前term的诊断历史：按准入限额、halt、pause的Go优先级记录门禁结果；
实际balance调度结束后归还同一recorder。读取仍走原调度命令队列，不另建worker。
[当前差分及取消证据](../../docs/rust/server-api/task-066.md)。
