# scheduler_restore.rs

`Coordinator::init_schedulers(need_run, cancellation)` 在宿主串行访问下恢复真实
`RegisteredScheduler`；`are_schedulers_initialized` 初始为 false。没有私有 runtime、
后台循环或重复控制器。宿主向 `CoordinatorInputs.scheduler_configuration` 提供完整
持久配置 JSON，恢复后从同一 SchedulerController 的 configuration 或报告的
valid_configs 发布配置给宿主自己的配置 provider；泛型 `Arc<C>` 不提供写入接口。
`need_run=true` 发布普通调度策略，false 发布 handler 策略；循环驱动仍归宿主。

只读 Go `coordinator.go:263-407` 推导：最多10次读独立配置；读后及逐项处理前检查
取消；按 scheduler type 找到旧列表首条，跳过 disabled/孤立配置；独立 JSON 优先，
然后旧 args 回退；重复添加不丢弃旧列表项；disabled 原样保留；无效旧项移除。
真实 factory 负责反序列化，真实 controller 执行 prepare/save/clean 与发布。
单项失败进入 skipped，继续处理；全部读失败传播类型化 ControlError（library 不
执行 Go 的 log.Fatal/退出进程）。取消不会撤销已经发布的项，与 Go 保留部分恢复
一致；宿主必须等待 future 完成，不能通过丢弃 future 代替 cancellation token。
最终完整配置保存失败进入 persistence_error，仍置 initialized，允许重试持久化。
无法解码的宿主启动列表返回 Config 错误；非 UTF-8 独立键报告并跳过，避免有损改写。

按用户授权采用源码推导与真实 Rust 行为/故障验证，不声称 Go 运行输出或运行差分。
固定 fixture 验证两种模式的20项恢复案例；另有18类内置工厂×两种模式36项真实
恢复、取消/10次读取失败/瞬时错误重试/最终保存失败、添加写失败清理和重启后实际
TransferLeader(region1,store1→3) 的结果。MemoryStore 是 canonical MetaStore 后端；
仅其 I/O 边界注入错误。无时间字段归一化，随机边界固定选择首项。
证据和命令见 [任务114](../../docs/rust/schedule-execution/task-114.md)。
