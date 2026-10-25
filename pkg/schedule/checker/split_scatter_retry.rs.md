# 拆分散射重试与回收

`SplitScatterRetryController` 在宿主串行调用下持有任务53的 canonical
`SplitScatterObserver`，身份不重复保存；另一个按 ID 索引的表仅保存 retry deadline
和 attempted。策略 deadline 来自同一宿主单调时钟；标签与 placement 使用 context.now 的 Unix 纳秒。
控制器不创建运行时、后台任务、
全局指标注册器或持久化后端；shutdown/cancellation 调用 `clear`。

`record_batch` 跳过 disabled/空批次，先回收过期项，再按4096容量原子登记。
TTL 为30分钟，新批次重置全局 dispatch deadline。容量统计保留 Go 对输入重复 ID
逐项计数的行为；登记本身仍由 observer 覆盖去重。结果版本、source 版本、group 和
包含 deadline 的身份检查复用任务53，旧快照不能延迟或删除替换后的身份。

`top_pending` 的零 limit 不触发回收，正 limit 先回收再遍历。结果缺失/source 缺失
会推迟10秒并计 missing；低版本继续等待，高版本删除；已有 `split-scatter-region`
operator 的项推迟10秒。候选按 expire_at、group、region ID 排序后截断，只有真正选中
的项标为 attempted。到期条件是 now >= expire_at；分别累计 attempted/unattempted，
删除和取消均更新 pending gauge 快照。

`dispatch` 总是先清理 TTL，再检查 disabled、全局 deadline 与正在运行的
OpSplitScatter 数量。没有候选或达到上限时设置10秒全局退避。
派发使用真实 RegionReplicationContext/placement fitting、RegionLabeler 和
ControllerAdmission 的存储令牌检查/准入，顺序执行使后续规划看到前项新 operator。
任务52的范围 hint 优先提供 scatter group；batch group 写入真实 operator additional info。
成功创建或无需 operator 才删除 pending，其余失败保留并延迟。

实际 scatter 算法属于后续 scatter 任务。这里由宿主传入规划函数，输入真实 region、
group、range hint 和实际 operator controller，输出真实 Operator 或类型化
`SplitScatterDispatchError`，不另建散射算法或伪造调度状态。测试在该边界注入
RetryLater/规划错误，验证 pending、deadline、指标和实际准入结果，不以调用次数验收。
HotRegion、BalancedReadCpu 单独计数，其它 retry/error 计 scatter_failed；准入错误保留
原 AdmissionError 并区分 store_limit 与 add_operator_failed。任务58增加
`set_metrics(CheckerMetrics)`，在每次真实变更时同步发布累计事件和 pending gauge；
原 `metrics()` 快照 API 保留，无全局状态。宿主须在接受工作前接线，见 [指标说明](checker_metrics.rs.md)。

来源是只读 `split_scatter.go` 与相邻测试，按用户授权以源码契约推导期望及 Rust
行为测试验收，没有声称运行 Go oracle。证据、fixture、失败日志和 Cargo 槽位见
[任务54证据](../../../docs/rust/schedule-execution/task-054.md)。
