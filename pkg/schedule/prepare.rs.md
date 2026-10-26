# 调度准备判断

`PrepareChecker` 属于 canonical `astersql-pd-schedule`；仅持有启动单调时间、
总区域数读取函数及 mutex 保护的 sticky prepared 标记。`BasicCluster` 的
`RegionsInfo`、`StoresInfo` 仍是唯一 store/region 状态来源；collect/reactive
通过已有 `set_region` + `update_sub_tree` 生效，不引入重复心跳集合。

`new(start, provider)` 接收宿主时间和 `Fn() -> Result<i64, E>`；`check(cluster, now)`
与 `should_run(cluster, now)` 共享同一判断，`is_prepared` 仅查询，`set_prepared`
保留 Go 测试 override。完整 Coordinator 装配由后继任务负责。

按 `prepare_checker.go`，已准备优先返回 true；严格超过五分钟直接准备；
读取总区域数失败返回 false 并可重试；全局非 storage 区域数至少达到总数的 90%；
对 Preparing/Serving store，非 storage 区域数同时满足 raw heartbeat region_count
的 90% 和 canonical index 区域数的 90%。其余 node state 跳过。
一旦准备，之后状态退化不撤销；重启须新建 gate。Sync 与 Heartbeat 都算 active。
保留 Go 的 f64 阈值运算和比较顺序，不将阈值截断成整数。

错误类型 `E` 留在宿主读取边界；Go 的 bool 准入接口会消费读取错误并拒绝本次，
此处保持相同语义，没有凭空构造 RFC 错误。读取函数不能重入 gate；锁覆盖整个判断，
调用方通过对 cluster 的借用或宿主锁协调区域更新。宿主必须传入同一单调时钟域，
不创建 runtime、计时任务、信号或全局状态。panic 表示程序错误，非可恢复 I/O 错误。

按用户后续授权，预期来自只读 Go 源码推导，不声明 Go 运行 oracle 或运行差分。
固定输入、来源摘要及实际红灯/绿灯、故障恢复和 workspace 证据见
[任务112](../../docs/rust/schedule-execution/task-112.md)，
[独立测试](task_112_test.rs) 显式注册于 `lib.rs`。
