# 均衡公共求解规则

`BalanceSolver` 借用 canonical `ScheduleConfig`，输入真实 `BalanceSchedulerPlan`、
`StoreInfo`、`RegionInfo` 和 `OpInfluence`。`from_cluster` 从 `SchedulerCluster`
的真实 region 索引取平均大小与最大 store region count；显式 `new` 支持调用方提供
范围集群配置/统计，不复制集群、不安装 runtime、不涉及协议或持久化。

`evaluate` 每次重新读取候选及在途量，返回 `BalanceScores`；只有 source 严格大于
 target 才允许迁移，等值及 NaN 都拒绝。source 的在途量强制非正、target 强制非负，
避免连续调度和回流。region 在途量放大 5 倍，两侧对称应用
`max(average_size * ratio, candidate_size)`。leader/witness 按 count 时只截断 ratio；
其它策略使用平均大小。ratio=0 时 leader/count 为5，其余为
`max(max_region_count * 0.005, 1)`。整数运算保留 Go 环绕语义，score 复用 core 中
leader/witness/v1/v2 公式；未知 resource 返回0。调用方应使用已验证的调度配置。

缺少 source/target 返回类型化 `BalanceError`，候选 region 可为空。求解没有外部 I/O；
`OpInfluence` 的缺失 store 查询沿用既有 API 的零值创建行为，不改变资源 delta。
本任务不迁移 retryQuota、scatter count threshold、HTTP 或 debug metrics。

Go依据为 `utils.go` 的 solver 及 `balance_region_test.go` / `balance_leader_test.go` /
`balance_witness_test.go` 的对应契约。用户已解除 Go 运行证据准入，冻结 fixture 明确为
源码推导，未宣称 Go 执行或运行差分。验证及边界见
[任务67证据](../../../docs/rust/schedule-execution/task-067.md)。
