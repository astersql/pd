# 过滤计数与指标清理

`FilterCounter` 是 scheduler/checker 独占的批次，使用前序 canonical
`FilterType`；`inc` 只累计内存计数。`set_scope` 影响下一次刷新，也影响尚未刷出的
旧计数。`flush` 消费批次，通过 `astersql-pd-schedule-core::BasicCluster` 查询同一宿主
的 canonical store registry，丢弃 missing/Removed store；传入 None 对应 Go nil informer。
批次消费后查询为零，不保留 Go map 中无业务意义的零值槽。

`FilterMetrics::register(&Registry)` 注册 Go 原名、help、Counter 类型和三个标签；
不增加 `_total` 后缀或标签维度。每个 Registry 的向量独立，clone 共享同一宿主。
注册冲突返回 prometheus 错误；第二个向量失败时撤回本次注册的第一个向量。
没有进程全局注册、runtime、protobuf 或 I/O。这里不自动接管 host 的周期任务。

缓存只在 flush 时创建，inc 热路径不调用 WithLabelValues。生命周期锁串行化刷新、
`delete_store_metrics` 和 `reset_filter_metrics`；删除覆盖两个方向、所有 scope/type，
同时销毁缓存句柄，使后续合法刷新重新创建可抓取的序列。宿主先发布 Removed 状态，
再删除指标；被计数后移除的 store 不会在下一次 flush 复活。删除不清 pending 批次，
reset 也只清指标，均沿用 Go 语义；None informer 不提供移除保护。
库内锁不替代宿主的 store 生命周期协调。无持久化操作，故不引入存储故障模拟。

来源：只读 `counter.go`、`metrics.go`、`counter_test.go`，commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`。26 个 filter label、两个方向、无效
上界的 unknown、负索引 panic 保留来源契约。Go `SetScope` 无调用者，被已有二进制
链接器消除；只读确认其单行赋值，以调试器设置同一 scope 领域输入，实际执行 Flush
冻结 pending 计数的 scope 迁移输出，不声称执行了消除的方法。

[任务证据](../../../docs/rust/schedule-execution/task-011.md)及
[冻结 fixture](../../../docs/rust/schedule-execution/fixtures/task_011.json)记录实际
Go 输出、来源哈希和归一化。独立注册的 `task_011_test.rs` 重放10组生命周期案例，
补充 Registry 隔离/冲突回滚、缓存重新创建、负索引失败和多线程独立批次共享指标。
