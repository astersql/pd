# 热点排名 v2

领域入口为 `HotRankV2`、`RankRatios`、`HotRankV2Result` 和
`v2_priority_cmp`，通过同目录 `lib.rs` 导出。`HotRankV2::new` 校验两个不同的
0..4 维度，返回 `HotRankV2Error::InvalidPriorities`。没有 I/O、私有运行时、
协议对象、全局状态或新的 feature。

`progressive_rank` 用真实 `HotSolution` 的去噪 peer 净负载、current/future 极值
和 pending 进行评分。调用者提供 eligible store 的 extrema、离散步长及源/目标
nth peer 速率；负向负载选择目标端的 top-n。写 leader 只看第一维，沿用 Go
不更新两个 score 字段的行为，返回初始化的零分。其他资源返回两个 score 和 rank。

评分保留 balanced/pre-balanced/unbalanced 的不同区间、闭/开边界、最小热速率、
反向容差和 pre-balanced 时 pending 总和必须严格小于 1 的限制。
`check_loads`、`check_history` 对普通资源任一维满足即可，对写 leader 只看首维。
历史样本必须对齐；短 expectation 和未初始化缓存保留 Go 的调用前置条件并 panic。

`better_than` 先比较 rank、revert 有无、源/目标 store，再按 rank 对两个维度择优。
store 比较复用批次 87 的 canonical 实现（只把其方法开放为 crate 内可见）。
同 score 为 2 时偏向大 peer，其余 score 偏向小 peer，保留离散桶和相同候选旧值。
`set_search_revert_regions` 更新调用者持有的资源级状态；`need_search_revert_regions`
检查当前与最优候选；`HotRankV2Result::is_available` 对 revert 候选要求 rank>=3。
指标发布由宿主负责。

`filter_uniform_store` 接收真实人口统计的标准差快照：主/revert scope 为 None 时
回退源 store 的全局标准差；revert scope 只对包含 revert 的候选生效。
作用域的规则匹配、人口计算和完整调度/operator 装配属于 solver，未在排名层重复实现。

来源为同目录只读 `hot_region_rank_v2.go`、`hot_region_solver.go` 及相邻测试。
按用户授权采用源码/测试推导 + Rust 真实行为/故障验证；没有执行 Go 排名测试，未声称排名运行差分。
固定 inputs/outputs 与 provenance、红灯和通过证据见
[任务088](../../../docs/rust/schedule-execution/task-088.md)。
