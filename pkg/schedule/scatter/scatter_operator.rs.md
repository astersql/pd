# 散射操作与分组记账

[实现](scatter_operator.rs)消费任务59的 `ScatterTarget`，通过 canonical
`RangeOperatorFactory` 构建 admin scatter；返回真实 `Operator`，不创建运行时或复制状态面。
宿主需要串行持有目标选择和计数提交这两个阶段；`&mut ScatterCounts` 保证提交阶段独占。

公开入口 `create_scatter_operator` 保留 allocator 的 `BuilderError`，包装为
`ScatterOperatorError::Build` 并提供 error source。成功时记录目标布局，设置 group、
leader-picked-count 和 HIGH 优先级。skip-store-limit 传入工厂：新增副本始终 light，
删除副本仅 skip=true 时 light；操作准入和限流由已有 controller 执行。

Go `scatterRegionWithType`/`Put` 的源码契约明确：相同 store/leader 布局无需操作，
仍记录当前布局；角色/数量错误或创建失败记录原布局，不把失败目标写入计数。
`record_distribution` 按 group/engine 隔离 peer，leader 使用普通引擎，跳过缺失 store，
leader=0 不记账；整数按 Go uint64 回绕。角色/witness 校验先于无操作判断。
本入口与任务59一样限定 admin 单区；内部 seeded scatter-state delta、CPU 策略、
批处理及 metrics 生命周期不由此入口承担。当前没有 MetaStore I/O。

依据用户授权使用只读 Go 源码/测试契约推导与 Rust 行为验证，没有运行 Go 或声称运行差分。
[证据与验证](../../../docs/rust/schedule-execution/task-060.md)记录失败变异、
正常/错误案例、共享 Cargo 槽位和全仓检查。测试独立在 [task_060_test.rs](task_060_test.rs)。
