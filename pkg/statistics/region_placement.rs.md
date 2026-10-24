# 放置规则驱动的区域分类

`RegionStatistics::observe_with_placement`借用 canonical `RuleManager`、
`CheckerConfigProvider`以及真实core快照，返回`Option<RegionAnomalyDurations>`。
`None`明确表示启用placement但manager尚未初始化，本次未观察；已有bit、反向集合与时间戳均保留。
禁用placement时不检查manager，继续使用配置max_replicas。

启用时直接调用`get_rules_for_apply_region`，总副本数为适用规则count之和，
voter数为非Learner规则count之和（包括Leader/Follower/Witness）。按Go有符号整数加法语义求和。
规则范围、override及分段全由已有placement实现决定，不重做fit/checker，不根据store标签过滤规则。
跨分段region的空规则列表给出0副本/0voter期望，不回退到配置，故有peer时产生ExtraPeer。

计数交给`region_classify.rs`内共享`observe_counts`，复用全部10类异常、替换/清除和duration逻辑。
MissPeer依据全部peer（含learner）；miss-voter duration仅在缺总peer且voter不足时累计。
更新规则或配置后下次observe读取新值；NeedUpdate仍保持Go触发子集。
原无manager的`observe`保持placement启用时显式返回`PlacementRulesRequired`，提示使用新入口。

宿主负责串行化索引和规则更新、供给同代配置与Unix秒时钟；不持有runtime、全局状态或后台任务。
规则管理器的存储属于上游依赖，本模块仅同步只读查询，无新后端协议。
独立`region_placement_test.rs`在lib显式登记，复用真实MemoryStore/StorageEndpoint与RuleManager，
包含Go oracle重放及learner、边界、override、规则更新、开关、未初始化、时间戳保留测试。
证据见[任务16](../../docs/rust/state/task-16.md)。
