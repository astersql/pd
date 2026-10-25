# 合并可行性判断

`allow_merge` 对两个真实 `RegionInfo` 范围检查非空共享边界，按合并后范围查询
canonical `RuleManager::get_split_keys` 和 `RegionLabeler::index`。placement 启用
但能力缺失时拒绝；labeler 可选，两个区域的 `merge_option` 仅精确小写 deny 拒绝。
标签时间由宿主传入，不新建运行时、后台任务或复制规则状态。

Table 模式关闭跨表合并时，以及未知 policy，无论 cross-table 配置如何，都要求
canonical codec 的完整 `TableIdentity` 相同，包括 has_keyspace、keyspace_id、table_id。
Raw/Txn 允许跨表但仍检查范围/规则/标签。codec 解码失败映射零 identity，与 Go
TableIdentity 的降级一致；此处不把无效 boundary 当成新错误返回。

`check_peer_store` 允许 source 已有的 missing/removing store；拒绝 target 新引入
的 missing/removing store。`merge_target_allowed` 组合上述判断与真实 filter 的
health/replication：legacy 精确副本数且无 learner，placement 使用真实 fit。
它支持 absent target，无副作用；placement 可更新自身 fit cache。宿主仍负责 hot/
recent-split 门控，后续任务负责完整 checker 生命周期、大小约束、选择和 merge operator。
本任务不创建操作、分配 ID 或写 MetaStore，因此不存在相应 I/O 失败路径。

来源：只读 merge_checker.go 的 AllowMerge/isSameTableIdentity/checkPeerStore/
checkTarget；merge_checker_test.go 的 TestAllowMergeCrossTable 和 suite 目标过滤契约；
codec.go/healthy.go/placement fit/labeler 边界语义。遵循用户豁免，本次无 Go 运行差分。
固定输入、来源哈希、失败识别和实际 Rust 验证见
[task-046](../../../docs/rust/schedule-execution/task-046.md)。
