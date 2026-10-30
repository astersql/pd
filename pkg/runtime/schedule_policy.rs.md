# Restored scheduler execution adapters

`PolicyInputs` supplies the canonical cluster, configuration, shared placement
manager, current HotStat snapshot, restored labeler, range manager, operator
admission, fenced endpoint and a reserved ID window to each concrete builtin.
`BorrowedPolicy` borrows the registered policy rather than creating a second
policy with the same name. It implements the existing LoopScheduler contract;
ScheduleController owns retry/backoff and label filtering, while management owns
the eventual prepare/clean pair.

The exhaustive builtin match supplies each policy's established typed context.
No protobuf, transport or etcd types enter domain libraries. Region and merge
builders consume IDs reserved by the same node allocator as AllocID; unused IDs
are abandoned normally. Policies cannot allocate an ID by blocking on an async
callback inside a builder.

Hot adapters prepare per-resource statistics using existing load projection,
source/destination filters, replication/placement guards, rank solvers and pending
admission. Rank population inputs refresh on the restored HotScheduler without
losing reverse-search state. This adapter does not replace those algorithms.
Per-policy errors remain observable and retry on a later tick.

The initial term restores the persisted RegionLabeler. Future dynamic label and
scheduler management surfaces must update these same owners; this task does not
expose those HTTP APIs. Runtime task-017 tests cover the scheduling chain with
restored eviction and cold defaults; existing domain regressions cover individual
policy algorithms. Do not interpret that as new hot-policy wire parity evidence.

任务66把同一协调器的诊断recorder借给本轮调度输入。leader/region balance
仅在首轮重试启用plan收集，并通过已有DiagnosticRecorder聚合实际operators和
plans；后续重试不覆盖该次记录。[当前差分证据](../../docs/rust/server-api/task-066.md)。
