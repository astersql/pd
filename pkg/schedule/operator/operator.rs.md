# Operator：心跳推进与完成

`operator.rs` 是 canonical Operator，实现 Go `operator.go` 的 Start、Check、
CheckSuccess、CheckTimeout、ConfVerChanged、UnfinishedInfluence、TotalInfluence
及优先级、健康检查标志。OpStep 枚举复用批次18–23的12种真实领域步骤；不复制步骤
判定，不纳入仅供联合共识内部使用的 DemoteVoter。命令和进度错误仍由具体步骤提供，
Controller、Builder、历史记录和HTTP序列化不属于本任务。

执行锁保护单调的 current_step、每步首次完成时间及总影响量缓存。多线程心跳安全；
终态不会重新推进，过期/取消/替换沿用已有 OpStatusTracker。Check 从当前步开始，
遇到第一个未满足步骤停止，同一心跳可跳过连续多个已满足步骤。完成优先于超时；
检测出超时的该次调用仍返回未完成步骤，后续终态调用返回 None。保持 Go 在 CREATED
状态也可以检查步骤但只能在 STARTED 后进入 SUCCESS 的契约。

ConfVerChanged 包含当前步，只求已推进前缀及当前步；空步骤安全返回0。未完成影响量
从当前步开始，跳过当前心跳已满足的步骤。总影响量首次收到 Some(region) 时缓存，
以后即使传入不同 region 或 None，也累加缓存；没有缓存时 None 不产生影响量。
这两种计算不会自行推进 Operator。

时钟是宿主提供的单调 Clock（默认 Instant），没有私有 runtime 或全局状态。
超时按 Go 的每步有符号纳秒→浮点秒求和→截断秒→有符号纳秒乘法计算；负值立即超时。
优先级默认 MEDIUM，含 ADMIN 时默认 URGENT，允许之后覆盖。健康检查标志默认 false，
可随时切换，Check 本身不进行 store 健康验证（与 Go 相同）。

Go 源码和相邻测试只读。按用户授权，证据为源码/测试契约推导，未运行 Go operator，
不称为运行差分。来源哈希、固定输入、红绿验证、并发与故障边界、Cargo槽位记录见
[任务024证据](../../../docs/rust/schedule-execution/task-024.md) 和
[固定fixture](../../../docs/rust/schedule-execution/fixtures/task_024.json)。
本对象不访问持久化或外部 I/O，无适用的 MetaStore 故障注入点。

任务25在同一 Operator 组合 AdditionalInfo，增加有原因取消与 merge Sync；
历史/记录/JSON 的扩展实现见 [operator_record.rs.md](operator_record.rs.md)。

任务099新增 `Operator::decorate(&mut self, desc, kind)`，用于范围散射在发布前独占
修改 description 并附加 RANGE 位；保留已有步骤、kind 位、priority、health 和附加信息。
行为由 schedulers/task_099_test.rs 的 leader 批次与 region 操作器案例验证。
