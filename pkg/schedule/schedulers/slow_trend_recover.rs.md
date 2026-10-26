# 慢趋势驱逐与恢复

`SlowTrendScheduler` 接续 [候选选择](slow_trend_candidate.rs.md)，使用 canonical
BasicCluster/StoreInfo、StorageEndpoint/MetaStore 和 EvictLeaderScheduler。
仅提供领域 API；不安装 runtime、HTTP、全局 scheduler 工厂或新 feature。

`observe` 返回本轮是否应驱逐：捕获候选后等待其它节点心跳，旧候选变快则取消；
确认候选后 pop(true)、更新内存驱逐列表、保存配置、获取 core 慢趋势驱逐标记。
已有驱逐节点只有在快比较满足且从**捕获**起累计整秒达到恢复阈值时退出；
删除/缺失节点直接清理，本轮不再捕获。候选计时复用上一任务实现，时间以显式
Unix 纳秒输入，宿主提供不回退时钟；schedule 的时间应与 context 时间一致。
一次 capture 的两次时钟读取都取当前显式 now；没有墙钟或随机期望归一化。

持久化字段沿用 Go `recovery-duration` 和 `evict-by-trend-stores`，默认1800秒，
重建不恢复候选历史；零历史的 readyForRecovery 采用 Go time.Duration 最大值
所对应整秒。reload 调整 incoming leader pause 引用而非慢趋势标记，prepare
获取首个节点的慢趋势标记，clean 释放该标记。候选缺失及驱逐节点缺失分别返回
类型化 MissingCandidate 或清理，避免源路径的 nil 解引用。

保留 Go 的故障顺序：驱逐 set/clear 写盘失败不恢复内存列表；clear 失败仍释放
core 标记并更新历史恢复时间；策略更新失败回滚。返回 storage/store 类型化
错误，schedule 当轮不发操作器。Rust clean 使用 Drop guard，宿主取消 I/O 时也
释放标记并更新历史；这是异步边界清理保证，不宣称有等价 Go 取消运行结果。

调度操作器复用已验证 leader batch=3，描述 evict-slow-trend-scheduler，
kind Leader、urgent 优先级和多目标 TransferLeader；目标过滤、confver、影响量
和 classic/raft-v2 heartbeat 命令均由共享构建器产生。无驱逐节点时允许观察，
有驱逐节点时比较 leader limit。next_interval 忽略旧间隔，从 minimum 执行
exponential/zero growth，与 Go 的实际实现一致。

期望依据是只读 `evict_slow_trend.go`/`evict_slow_trend_test.go`，按用户授权采用
源码/测试推导，**无 Go 运行 oracle 或运行差分**。
[任务证据](../../../docs/rust/schedule-execution/task-084.md) 记录固定 fixture、
实际红绿验证、五个独立测试及 workspace 检查。
