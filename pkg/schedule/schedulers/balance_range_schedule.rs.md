# 范围均衡调度

`BalanceRangeScheduler` 配合 canonical `BalanceRangeJobs` 队列工作。
`check_job` 按 OpRange 数量/region limit 准入、GC、开始、取消清理、超时完成、
prepare 和均衡完成依次执行。调用方在同一领域访问序列中调用 check_job 后再 schedule，
持有队列独占访问直到保存结束；HTTP、周期调度、操作器提交和指标注册由宿主装配。

`prepare` 使用真实 store 状态、engine、范围角色计数和 `get_count_threshold` 的
placement/topology 契约；仅两个及以上期望大于零的 store 才能准备成功。
计分保留原始整数计数；排序叠加范围内 operator influence，均衡判定逐 store 截断
绝对差后累加，不能换成浮点总差。宿主传入的 influence 必须限定于作业范围。

`schedule` 复用真实 StoreState/SpecialUse、down/pending/snapshot/replicated、
PlacementGuardContext、StatisticsSnapshot 和 PeerOperatorFactory。
来源按降序选择，peer 的 follower/leader/learner 顺序固定；目标从尾部选择，
必须低于期望且移动后的分数满足 `source-abs(influence)-1 >= target+abs(influence)+1`。
已有目标 peer 的 leader 作业创建 leader transfer；否则按旧 peer role 创建 move 或
replace leader peer。操作保留 RANGE kind、默认 Medium、四个两位小数评分字段。
随机采样、peer ID 分配、时钟由宿主显式传入，分配错误作为 BuilderError 返回。

`report_at` 每十秒发布 RangeReport 快照，宿主将其投影到指标系统。清理删除范围并
清零当前 score/expect；保留 Go 的 job gauge 和报告时间语义，不在领域层注册全局指标。
Go 超时后仍继续 prepare，未均衡时返回 true；随后均衡时再次 finish 返回状态错误，
此源行为被保留并测试。prepare 失败也保留已开始作业和已登记范围。

来源为只读 `balance_range.go` 和 `balance_range_test.go`，采用用户授权的源码推导，
没有 Go 运行 oracle/差分声明。证据、归一化、红灯/绿灯和限制见
[task-105](../../../docs/rust/schedule-execution/task-105.md)。
