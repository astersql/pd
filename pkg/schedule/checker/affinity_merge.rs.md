# 亲和组合并

`AffinityChecker::new(host_unix_nanos)`记录启动时间；宿主提供时钟、当前region存在性、
真实邻接快照和operator完成通知。`Default`以零为启动时间，适合固定时钟或已过启动期的
宿主。`check_with_neighbors`返回空、一个副本修复operator或两个合并operator。
原有`check`保持仅修复的兼容接口；两者共享`prepare_group`与`best_location`，避免
分叉亲和组观察、归一化和缓存失效行为。

亲和合并等待启动60秒，拒绝成功合并缓存、禁用限额、过大source。先验证next，再按
单向配置决定是否考虑prev，较小prev获选，相等保留next。候选必须处于同一允许调度的
完整亲和组、已经亲和、真实相邻、健康、peer store安全，并满足placement和最佳位置。
组合范围内placement/label split keys阻止合并。热点、近期拆分、table identity和
`merge_option=deny`不参与本路径。

选定后才判断目标缓存和总大小/keys，不回退到第二候选。keys阈值是亲和大小的10000倍；
全局merge大小和调度限额只用作启用门禁。使用canonical RangeOperatorFactory，保留
输入epoch、关联ID和主动/被动MergeRegion步骤。Go factory接收OpAffinity，故纯merge
kind仍为AFFINITY；需要learner对齐时factory添加REGION。构建失败返回BuilderError，
不返回部分操作。成功回调复用record_op_success，不在生成或准入时提前缓存。

依据只读`affinity_checker.go`及相邻Go测试推导契约，没有声称Go运行差分。38个冻结
Rust案例、真实MemoryStore/Manager/RuleManager/RegionLabeler和factory、controller成功
回调及错误变异证据见[任务50](../../../docs/rust/schedule-execution/task-050.md)。
