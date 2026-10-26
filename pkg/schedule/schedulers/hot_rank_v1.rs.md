# 热点排名 v1

`hot_rank_v1.rs` 提供 `HotRankV1`，复用任务86的 `HotSolution`、真实
`StoreLoadDetail` / `StoreLoadPred` 和任务69的 `rank_cmp`。入口由
[lib.rs](lib.rs) 公开；测试显式登记于 [task_087_test.rs](task_087_test.rs)。

调用者传入配置快照、已解析的两个维度、write-leader 标记，以及同一轮候选总体的
maxSrc/minDst/rankStep。优先级须不同且在0..4，否则返回 `HotRankV1Error`。
此层不选择总体、不构造 operator、不访问存储、不发布指标、不安装运行时。
write-leader 与普通资源的区分由宿主完成；配置校验继续使用任务85契约。

`progressive_rank` 每次重算选定维度的净peer负载，保留有符号最小速率早退、
负向净负载的极值反转、pending放大容差、两维改善矩阵和write-leader交叉边界。
`check_loads` / `check_history` 区分source/destination的严格不等式；
write-leader或非严格筛选仅检查第一维，严格筛选按维度索引检查两个维度。
空历史维度通过；样本期望短于输入时保留Go索引panic前置条件。

`better_than` 按rank、revert存在性、source、destination、peer依次选择。
源使用min/current-future、目标使用max；负载相等再比较pending差值。
write-leader先比较pending peer count，然后两维差值；其它资源只比较第一维差值。
peer比较使用100/10/10/10的步长与向零截断；write-leader直接比较第一维速率。
同一peer对象保留旧候选；split rank>=5直接获胜。沿用HotSolution结构约束：
可选revert peer表示完整的revert候选；本层没有独立的Region状态副本。
步长来自Go对应总体计算；离散排名的正常输入是有限负载及非零步长，
不承诺Go对NaN/无穷转整数的实现相关结果。

`filter_uniform_store` 使用0.05/0.1严格阈值和配置expectation开关。
Go `getPlacementLoadState` 对v1关闭placement-v2总体限制，因此使用全局stddev；
本层不接入v2 scope。`need_search_revert_regions` 恒false，更新搜索状态为空操作，
但仍保留传入已有revert方案的净负载与择优语义。

来源为只读 `hot_region_rank_v1.go`、`hot_region_solver.go`、`hot_region.go`，
按用户授权采用源码推导期望加真实Rust测试。没有执行Go测试或声称运行差分。
[任务证据](../../../docs/rust/schedule-execution/task-087.md)保存provenance、
15组冻结fixture、错误实现红灯、6个测试及workspace检查。
