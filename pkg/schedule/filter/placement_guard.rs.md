# 位置与规则保护

[placement_guard.rs](placement_guard.rs)在 canonical filter crate 中实现
DistinctScoreFilter、RuleFitFilter、RuleLeaderFitFilter、RuleWitnessFitFilter、
PlacementGuardContext 和 create_region_for_rule_fit。

DistinctScoreFilter 冻结位置标签、排除全部 source ID 后的 store 快照与原位置
得分；Safeguard 接受相等，Improve 必须严格改善。Source 始终成功，Target
失败保留 STORE_NOT_MATCH_ISOLATION。空 store 输入保留 Go 构造器的 panic，
不比较语言特有的失败文字。规则过滤复用 placement 的
RegionFit::replace，不复制匹配、角色交换或隔离算法；返回
STORE_NOT_MATCH_RULE，DetailedReason 与 Go 一样为空。filter identity 分别
为 2、4、5、4，scope 保留调用者输入。

Leader 允许移动时可选择 region 外的 store；已有 witness 始终拒绝。
Witness 只允许已有且 peer ID 不等于当前 leader 的 peer；无 leader 的行为与
Go nil-safe GetId 一样使用 0。未知 source、约束冲突、隔离退化和缺失目标按
真实 RegionFit 和 membership 返回拒绝，不增加异常或隐式修复。

PlacementGuardContext 仅在构造时借用宿主 config、stores、RuleManager。
已有 Arc<RegionFit> 直接复用，否则调用 manager.fit_region，时钟由宿主显式
提供以复用既有 cache 健康判断。关闭规则时 peer guard 使用位置保护，leader
与 witness 返回 None，对应 Go nil filter。运行过滤不访问持久化、修改缓存、
创建 runtime 或保存上下文。既有 placement -> storage-endpoint -> proto 依赖
链保持原样，公开 API 只用领域类型。

临时 region 复制 key range 与 leader，逐一重建 peer 的 id/store_id/role 并清除
peer witness，随后按顺序应用 canonical RegionOption。leader 独立完整复制，
不随 peer witness 清理；region ID、epoch、health 与统计使用构造器默认值。
修改临时 region 不会影响原 region 或输入 peers。没有全局状态或新 feature。

[任务证据](../../../docs/rust/schedule-execution/task-009.md)记录 Go 运行
oracle、129 组/258 次返回、临时 region 返回、真实内存 MetaStore 的工厂测试、
错误行为变异与 workspace 回归。期望来自冻结 Go 返回；源码只解释额外所有权
与选项组合性质，不能当作运行差分。过滤器无 I/O，不制造无关存储故障；错误
场景包括缺失 source、缺失 target、leader/witness 禁止角色与规则/位置退化。
