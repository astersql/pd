# label_query.rs

`RegionLabeler` 查询入口复用 `LabelRuleIndex::get_range_rules` 与真实 core
`RegionInfo`，跨任何内部边界时无标签。时间由调用者显式传入，TTL 到期瞬间仍有效。

- `get_region_label`：高 index 优先；同规则重复 key 取最后一个。负 index 有效。
- `get_region_labels`：每个 key 高 index 优先；同 index 保留首个，包括同规则重复 key。
  返回的 `RegionLabel` 只有 key/value。不同规则相同优先级的遍历次序无契约。
- `schedule_disabled`：`schedule` 值忽略大小写匹配 `deny`。
- `get_label_rule`、`get_label_rules`、`get_all_label_rules`：克隆并过滤过期标签；
  无有效标签则不返回。ID 查询保留请求顺序、重复 ID，忽略缺失 ID。
- `iterate_label_rules`：遍历原始规则（含过期标签），回调 false 立即停止。

查询不写存储、不清理权威索引、不启动后台任务，不暴露协议类型。
