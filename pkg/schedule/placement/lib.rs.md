# lib.rs

canonical astersql-pd-schedule-placement 入口，重导出 Rule/RuleGroup/GroupBundle/PeerRoleType、LabelConstraint/LabelConstraintOp 和两个匹配辅助函数。公开 rule/label_constraint，json 私有；显式注册独立 placement_values_test.rs。集中 workspace 依赖，默认 features，无私有 runtime。

测试与 Go oracle 见 [批次 21](../../../docs/rust/schedule/placement/21-证据.md)。
