# StoreStateFilter

对应只读 Go `filters.go:315–600`。`StoreStateFilter::source/target` 保留 leader
优先、首失败短路、source/target 条件差异、`plan::Status`、完整详细原因以及 Go
`filterType` 数值。`reason()`/`filter_type()` 返回最后一次条件的身份；每个通过的
条件重置为 10，没有启用条件则保留先前原因（零值初始为 excluded=0）。

领域边界使用已有 StoreInfo、SharedConfigProvider、PriorityLevel、真实 v1/v2
StoreLimiter。宿主为一次调度传入 `now_unix_nanos`，不读进程全局时钟。快照只检查
sending/receiving；pending 上限 0 表示不限制。保留 Go uint64→int64 转换语义。
raw network slow trigger/slow score 本身不会拒绝 leader；已有暂停和驱逐计数负责
相应安全门槛。scatter/fast-failover 放宽 peer 预算、snapshot/pending 条件，但
不会绕过先执行的 leader 条件或基本健康检查。

已有 `Filter` 是同步接口，真实 `StoreInfo::is_available` 是异步接口。直接阻塞
future、私建 runtime 或伪造预算快照都会破坏已有契约。本任务提供对象安全的
`AsyncFilter`、显式 `SynchronousFilter` 借用适配及 `select_stores_async`。
新入口依次 await 真实预算检查，并保留 candidate Arc 身份、输入次序、首失败
collector 状态及 unavailable-target step=2。同步入口保持原 API。后续异步调度
消费者应使用新入口；本地 filter 实例包含 Cell 原因，future 不承诺 Send，不应
跨线程共享这个可变诊断实例。库不创建 runtime、不持有全局状态、不执行外部 I/O。

预算查询不扣 token，Urgent 也检查 leader 入站预算；控制器自身的 Urgent
豁免不属于本过滤器。temporary 仅跳过临时状态，不能跳过 removed/down、暂停、
慢盘/趋势/停止驱逐或 reject-leader。空 limiter 在实际查询时沿用 core 的 panic；
先遇到硬拒绝时不会查询它。collector 的资源/step 不匹配沿用 plan 的类型错误。

Go 运行来源、243 组固定输入、486 项输出、差分和故障证据见
[任务证据](../../../docs/rust/schedule-execution/task-007.md)。测试在独立
`task_007_test.rs` 中显式注册；源码推导补充的精确时钟/整数边界与实际 Go
运行差分明确分开。没有 placement、storage 或 server/runtime 的替代实现。
