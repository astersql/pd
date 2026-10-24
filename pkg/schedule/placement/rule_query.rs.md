# 管理查询与适配入口

`rule_query.rs` 为 canonical RuleManager 增加只读视图：

- `get_all_rules`、`get_rules_by_group`：返回独立 Rule 克隆；Go Clone 清除运行时 group，因此按 group ID、rule index、rule ID 排序。
- `get_rules_by_key`：克隆已按真实 group priority 排序的范围列表，保留列表顺序。
- `get_rules_for_apply_range`、`get_rules_for_apply_region`：返回不可变 Arc 快照；跨度跨越边界时为空，与缓存/无缓存 FitRegion 共用入口。
- `get_split_keys`：复用 RuleList/rangelist，边界严格位于开区间内，空 end 为正无穷。
- `get_rule_config_for_reconcile`：规则按 `(group_id, id)`，非默认组按 ID；规则保留 Arc 身份，组是独立值；后续写入不改变旧快照。
- `get_rules_count`、`get_groups_count`、`may_restrict_store_load`：读取已提交配置及其引擎汇总。
- `get_rule_group`、`get_rule_groups`、`get_group_bundle`、`get_all_group_bundles`：组按 `(index,id)`，bundle 的规则值快照保留原始 group 元数据；缺失组返回 None，缺失 bundle 保留请求 ID、默认优先级及 None rules；存在但无规则的显式组同样保留 None rules（Go JSON null）。

宿主通过 `&mut RuleManager` 串行发布变更，Go 的 Locked 查询别名无需第二套接口。
Go 两个标注 Only used for testing 的 placeholder/direct cache 方法不构成生产查询契约，Rust 使用真实缓存行为测试。
不新增存储操作、协议类型、时钟、任务或依赖。真实 MemoryStore 初始化/更新后查询，
已有跨后端持久化契约由批次 24/25 保证；本批次无新增 I/O 故障边界。

独立测试显式注册为 `placement_manager_test`；证据见
[批次 30](../../../docs/rust/schedule/placement/30-证据.md)。
