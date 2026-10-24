# lib.rs

canonical astersql-pd-schedule-placement 入口，重导出 Rule/RuleGroup/GroupBundle/PeerRoleType、LabelConstraint/LabelConstraintOp 和两个匹配辅助函数。公开 rule/label_constraint，json 私有；显式注册独立 placement_values_test.rs。集中 workspace 依赖，默认 features，无私有 runtime。

测试与 Go oracle 见 [批次 21](../../../docs/rust/schedule/placement/21-证据.md)。

批次 22 增加公开 rule_list 模块和重导出 compare_rule、prepare_rules_for_apply、
check_apply_rules、BuildRuleListError、RuleList，显式注册 placement_order_test.rs。
范围算法复用 astersql-pd-schedule-rangelist，公开接口保持协议无关。
见 [规则列表说明](rule_list.rs.md) 和 [批次 22](../../../docs/rust/schedule/placement/22-证据.md)。
