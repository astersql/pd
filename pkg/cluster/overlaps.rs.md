# 覆盖区域清理桥接

`handle_overlaps` 是 `pkg/cluster` 的 canonical 桥接，接收被 core 淘汰的
`Arc<RegionInfo>` 列表与宿主已有的 statistics、label、placement、可选 affinity
状态。它只使用 ID，不重读 root，不删除 core/MetaStore，不移除热点历史，也不观察新区域。
`RegionStatistics` 和 affinity 可独立缺省，label 和规则管理器必须由宿主提供。

每个区域开始前检查宿主取消输入。一个区域的同步清理不包含 await 或进一步取消点，
完成后才进入下一个 ID；`OverlapsCancelled { processed }` 保留已完成前缀，
空列表即使已取消也成功。统计成员立即删除，label 只标记，稍后由既有
`clear_defunct_regions` 扣减。placement 缓存失效与 affinity 成员/当前版本计数
由真实领域 API 完成，重复 ID 和重复清理幂等。

`clear_overlap_statistics` 共用于主题 5 的 opt-in heartbeat 组合及本桥接，
防止复制异常统计/label 清理算法。Go 在统计删除后、label 标记前清 affinity；
Rust 将统计和 label 标记放在共用 helper，接着清 affinity、placement。
这些操作在宿主同一独占状态锁下执行，均不失败、不 await、不暴露中间结果，
因此观察边界与逐区域取消边界相同。

`server/cluster/heartbeat_effects.rs` 的现有 HandleOverlaps effect 调用本桥接。
`HeartbeatEffectsState.affinity` 持有宿主安装的 canonical manager；None 表示未安装。
不创建第二份管线、缓存或 runtime。StatePipeline 原来的 region/region_with_witness
通过同一个 submit_region_heartbeat 入口自动获得清理。取消仍映射为既有
HeartbeatEffectFailure::Cancelled；runner 拒绝任务仍为 Admission，已接受的 root
不会回滚。宿主负责对 manager 的其他修改进行同一状态锁的串行化。

Go 来源是 `pkg/cluster/cluster.go::HandleOverlaps`、`pkg/statistics/region_collection.go`
和 placement/affinity 的 InvalidCache。按用户明确授权，本任务以只读源码推导加 Rust
真实行为和故障验证验收；没有 Go 运行输出，不声称运行差分。相邻 cluster_test.go
只有热点 benchmark，不提供本任务的 overlap 行为断言。

独立测试 [task_128_test.rs](task_128_test.rs) 使用真实 RegionStatistics、LabelStatistics、
RuleManager、Manager、RegionLabeler 及 MemoryStore/StorageEndpoint。冻结场景和
测试/故障/槽位证据见 [task-128.md](../../docs/rust/schedule-execution/task-128.md)。
清理本身没有存储 I/O，不添加虚构存储故障；集成测试覆盖 runner 停止这一真实外部边界。
