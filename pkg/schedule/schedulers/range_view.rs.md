# 范围视图、范围计数和重试配额

`range_view.rs` 是 schedulers canonical crate 的领域实现，对照
`range_cluster.go` 和 `utils.go`。所有期望由只读 Go 源码/测试推导，Rust 实测，
未声称取得 Go 运行输出或运行差分；完整证据见
[task-068](../../../docs/rust/schedule-execution/task-068.md)。

`RangeCluster::new` 借用真实 `BasicCluster` 契约，将扫描所得的相交 region Arc
放入真实 core 根索引与角色子树。部分相交 region 计入完整大小。仅 Go 明确重载的
store 查询、leader/follower 抽样、平均大小和网络慢分数读取局部索引；其它根查询及
store 控制保持宿主语义。`local_regions` 暴露只读局部角色统计，`parent` 继续提供宿主
调度服务。视图实现 `BasicCluster`，不复制完整 SchedulerCluster 的配置、存储和工作器。

store 统计保留原压缩放大系数，先截断整 MiB 再乘单位；Available 使用 u64 环绕减法。
UsedSize 为零时保留原 Arc；其余投影使用独立 raw stats，保留元数据、权重和未重载的
learner/witness 计数，不改变父集群快照。正常统计的放大系数为有限正数；Go 对
浮点数越界转无符号整数的架构相关结果不作为兼容承诺。Rust 保留安全数值转换，
本任务实测包括有限压缩比、零使用量、MiB 截断和 Available 下溢。

`set_tolerant_size_ratio/get_tolerant_size_ratio` 提供独立覆盖，零值使用调用者传入的
当前宿主配置。`BalanceSolver::from_range` 将该覆盖与局部平均 region 大小接入已验证
求解器；全局根 store region count 的自动比率回退仍保持 Go 的嵌入行为。

`get_count_threshold` 接受同一发布代的显式 ReplicationConfig 和真实 RuleManager，
复用 core 拓扑权重与 placement label constraints（含 engine 排他标签）。禁用规则时
leader 使用原始范围数量；peer 使用 max replicas 和拓扑权重。启用规则时按真实规则
分段，按 leader/voter、learner 或所有角色筛选，剔除 Removing/Removed stores，
逐段累加。某段没有可匹配 store 时仅该段为零。

`get_key_ranges` 返回 canonical core 字节范围；按对进行 query-unescape，支持 NUL、
非 UTF-8、加号/空格和任意合法边界，忽略最后单个参数，无参数时返回全范围。
错误保留参数编号与百分号的字节偏移，不返回部分成功结果。

`RetryQuota` 按 store ID 延迟登记预算10，除2衰减并限制最小1；reset 恢复10，GC仅保留
指定ID，空集清空。接口不持有 store 对象，沿用来源的ID身份语义。

测试独立注册于 `task_068_test.rs`，使用真实 core、真实 placement RuleManager、
真实 MemoryStore；仅随机输入通过 core 既有回调控制。所有生产接口均为纯读取或局部
状态变化，没有新增 I/O、任务、时钟或私有运行时，因此故障验证是非法转义、缺失store、
空范围/匹配集及数值边界，不制造无关的持久化故障。
