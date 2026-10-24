# 查询标识（批次 31）

公开 `parse_group_ids`、`GroupStateProvider`、`collect_all_group_states`、
`collect_group_states`。空 raw 返回 None 对应 Go nil；非空但全空白返回 Some(empty)。
裁剪后按首次出现顺序去重，验证失败返回整体错误；空 raw 收集全部状态，
全空白 raw 收集空映射，未知 ID 被跳过。映射采用 BTreeMap 使 JSON key 顺序稳定，
列表保留调用方顺序。provider 返回拥有的快照，不泄漏 manager 锁或协议类型。
