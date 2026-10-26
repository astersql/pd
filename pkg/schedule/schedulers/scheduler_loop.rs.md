# 调度循环与停止

canonical 实现为 `scheduler_loop.rs`。`ScheduleController<S: LoopScheduler>` 接受真实
策略、host 的 `Arc<BasicCluster>`、时钟与 `LoopControl`，复用 core store/region、
labeler、placement、operator admission 和限额推进；不拥有私有 Tokio runtime。
`WitnessLoopScheduler` 和 `EvictLoopScheduler` 直接调用已有具体策略，保留类型化
builder/event 错误。其余策略使用同一个 LoopScheduler 契约装配各自已存在的 typed
context，不能把 BaseScheduler 当成具体策略。工厂注册和管理配置仍由 task107 API
负责；宿主必须将同一策略实例移交给执行 owner，串行处理 reload/配置命令。此文件
不增加第二份工厂或配置状态面。诊断 recorder/聚合属于 task109；diagnostic 参数
只在每次 Schedule 的第一次尝试转交策略。

## 来源与行为

只读依据为 `scheduler_controller.go` 的 runScheduler、Schedule、AllowSchedule、
cacheCluster、Stop、Wait 和 CheckTransferWitnessLeader；`operator_controller.go`
的 AddWaitingOperator；`core/region.go` 的 NeedTransferWitnessLeader。
用户明确允许以只读源码推导 + Rust 真实行为/故障验证替代缺失运行 oracle。
没有声称运行 Go 或获得 Go/Rust 运行差分。

- gate 顺序为策略限额、全局 halt、暂停到期；严格 now < delayUntil。
- 最多10次重试，取消在每次尝试前检查；每次重建 store 列表快照，Arc store
  payload 不深拷贝，单 store/region 查询仍使用 canonical 当前状态。
- 非空结果先恢复 min interval；任一 region 消失则丢弃整批重试；label deny
  逐项剔除，evict-leader 获豁免。耗尽重试后用策略 next_interval 退避。
- 第一个 tick 等待完整 min interval；允许的一轮结束后按最新 interval 重置。
  gate 拒绝不重置。等待批次按单操作或相邻 merge pair 入队；成功入队多少批次就
  调用多少次既有 promote_limited_waiting，不无界清空先前等待队列。
- witness event 只接收 witness leader，并使用既有10000容量队列的 try_send，
  full/disconnected 返回 false；单次具体策略消费最多3条，坏事件不会挡住后续好事件。

## 生命周期与边界

`run` 是可由宿主直接 await 的 future；`start_local` 使用现有 LocalSet，返回
`ScheduleTask::{stop,wait}`。stop 幂等；wait 不发出取消而等待策略自然退出，返回前
完成 clean。drop handle 请求取消但不终止任务；宿主必须继续驱动 LocalSet 并显式
wait，不能通过销毁 runtime/LocalSet 或 abort 代替优雅关闭。父取消用
`LoopControl::from_parent` 建立 child token，子 stop 不会取消父或兄弟任务。
正在进行的策略不被取消分支丢弃，完成本次工作后退出并 clean，与 Go 相同。
每个执行实例必须已经 prepare；run 对它执行一次 clean，宿主不能同时在管理
owner 重复 clean。eviction 的 clean 释放其准备阶段取得的真实 store pause 引用。

策略返回 typed error 时退出循环并 clean；调度/入队 future panic 时先捕获unwind、
执行clean再resume_unwind，宿主wait得到JoinError，不在库中退出进程；admission 拒绝保留在 LoopExit 并继续运行。
非正 interval 在启动前或轮末拒绝，返回 invalid_interval，仍 clean，不构造非法 timer。
LoopExit 保留实际策略 owner、错误与 cleanup 结果，宿主负责处理错误、实际 dispatch
和后续生命周期；不在这里重新实现 operator dispatch/history。

## 验证

独立 `task_108_test.rs` 显式注册；固定四行 gate fixture 加真实 witness/eviction、
RegionLabeler+MemoryStore、placement 与 ControllerAdmission。使用 host 的 Tokio
暂停时钟控制 timer；唯一延迟注入是外部 I/O 边界，真实调度和 store cleanup 仍执行。
测试验证操作内容、真实队列/活动操作器、store 引用、退避/重置、错误、取消与 Wait。
详见 [task108证据](../../../docs/rust/schedule-execution/task-108.md)。
