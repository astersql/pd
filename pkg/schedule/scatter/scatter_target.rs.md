# 单区域散射目标选择

`astersql-pd-schedule-scatter` 的 `ScatterTargetSelector::select` 迁移
`region_scatterer.go` 中普通/admin 单区域散射的目标规划部分：
`scatterRegionWithType` 的引擎分组/顺序规划、`selectNewPeer`、`allowLeader`、
`selectAvailableLeaderStore` 和角色/数量验证。输出 `ScatterTarget`，不生成或接纳操作。
前置复制完整性、热度、affinity 和已有操作准入由调用方负责；内部 split-scatter 的
请求状态构建、CPU 压力/暂停源处理、内部差距阈值不属于这个 admin 选择入口。

输入复用 canonical `RegionInfo`、`StoreInfo`、配置快照、规则管理器已解析的
有序 apply-range `Rule` 集合，以及宿主持有的 `ScatterCounts` 历史快照。
即使关闭 placement，仍须提供默认规则用于 leader 资格判断，与 Go RuleManager 一致。
`ScatterCounts` 按 group/engine/store/peer-or-leader 隔离，缺失为0；选择只读历史，
不新增第二个集群状态面、后台任务、随机源、存储后端或运行时。
操作生成/组记账由后续任务60复用这个输出接线。

- 普通 peer 先于 TiFlash peer；同组 peer 顺序及 stores 顺序显式提供，固定 Go map
  的一种合法迭代次序。TiFlash 的独立计数只影响其 peer 选择，不能成为 leader。
- 所有 store 的组计数形成 max/min；不全等时排除最高计数；选择最少被选中的可行
  store，计数相等保留原 peer。候选完全被过滤时也保留原 peer。
- 真实 `StoreStateFilter` 使用 MoveRegion/ScatterRegion/HIGH，真实 `ExcludedFilter`
  排除已选及调用方额外避让目标；源回退可能仍处于避让集合，这不是强制搬离接口。
- placement 开启使用真实 `RuleFitFilter`，关闭则用 `DistinctScoreFilter` safeguard。
  每次移动更新请求私有 peers 及独立 leader，后续选择重新 fit 当前布局。
  不修改共享输入，不拿旧 fit 接受多个挤入同一位置的目标。
- 选中另一个原始 peer 时保留该 peer 的原角色/ID/witness，预留其位置，然后给当前
  peer 重新选择，避免将 learner/witness 覆盖成 voter。新位置 peer ID=0，交给真实
  operator allocator；原位置保留 ID。
- leader 只从普通、非 learner/demoting、非 witness，且旧 fit 的 voter/leader 非
  witness 规则允许的 peer 对应目标中选择；组 leader 计数最小，平局保留输入顺序。
  `leader_picked_count` 返回选择前的组计数。
- 缺 leader、缺源 store、无 leader 候选、非法布局分别返回类型化错误；输出检查每种
  role/witness 数量及 store 唯一性，规划成功或失败均不提交组记账。

Go 文件只读。根据用户授权，期望由 Go 源码与相邻测试契约推导，**没有 Go 运行差分**。
实际源码/fixture SHA256、输入默认值及命令/退出码见
[任务证据](../../../docs/rust/schedule-execution/task-059.md)。

5个显式注册的 Rust 测试覆盖18个fixture案例、4种连续移动 placement 组合、预留角色、
真实 RangeOperatorFactory 消费、2个非法布局以及固定迭代顺序/重复确定性。
两次行为变异分别破坏重拟合及计数选择，真实断言失败后恢复；不把缺API编译错误当成
行为证明。MetaStore 不在这个只读规划入口发生 I/O，没有伪造存储故障覆盖。
