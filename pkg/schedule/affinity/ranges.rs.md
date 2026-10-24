# 亲和范围与标签映射

`ranges.rs` 对应只读 Go `txn.go` 的纯范围操作；不包含 Manager、事务或持久化。

公开类型 `GroupKeyRanges` 复用 canonical `keyutil::KeyRange`；常量
`LABEL_RULE_ID_PREFIX`、`LABEL_KEY` 固定标签命名。
`get_label_rule_id`、`parse_affinity_group_id_from_label_rule` 保留首个标签匹配语义，
不额外验证组 ID。`make_label_rule` 返回未调整的 canonical `LabelRule`，
空范围 data 为 null。调用 labeler 的 `check_and_adjust` 后才能提取已验证范围。
`extract_key_ranges_from_label_rule` 对缺失/非亲和规则返回空值，对未调整数据报类型错误。
`parse_key_ranges_from_data` 跳过 None，按原始 hex 字段解码而非信任派生 byte 字段。
`decode_hex_key` 接受大小写 hex 和空端点，保留 PD 错误码及消息。
`apply_remove_ops` 按 start 的完整字节值匹配并核对 end，保留剩余顺序及当前组 ID；
重复移除 start 报 already-exists，缺失或 end 不符报 not-found。
`check_key_ranges_overlap` 保留 Go 相同 start/end 优先判重与空端点规则。
`validate_no_overlap` 按 start 排序后检查相邻项，区分组内与跨组错误。
相同 start 的多冲突输入，Go 非稳定排序的组名选择不构成稳定次序契约。

独立测试在 `affinity_ranges_test.rs` 显式注册；运行证据见
[批次32](../../../docs/rust/schedule/affinity/32-证据.md)。
