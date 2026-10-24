# 通用标签范围索引

`LabelRuleIndex` 复用 canonical `LabelRule` 和 `rangelist::Builder/List`。
领域接口不依赖协议、存储或 runtime；输入规则须预先 `check_and_adjust`。

公开 API：`new`/`default`、`rule`、`rules`、`set`、`delete`、
`get_split_keys`、`get_range_rules`、`min_expire`、`remove_expired_labels`。

索引拥有规则，禁止通过引用改写派生数据。set/delete 在返回前完整刷新范围与
最早过期时刻，相当于 Go 写锁内修改后 Unlock 发布的快照。重复 ID 后者覆盖；
不存在的删除为 no-op。空 end 表示无穷，split keys 严格位于查询区间内部。
跨越任一边界返回 None，无规则但未跨边界返回 Some(empty)。同一规则的重叠
范围保留多次出现，与 Go builder 一致。返回规则次序无业务含义，index 较大者
优先；同优先级冲突的区域标签选择留给批次 19，不增加 ID 决胜规则。

过期过滤使用调用者时间，边界相等仍有效；过滤不删除规则，清理由调用方决定。
键空间专用分区/稀疏加速留给批次 16；当前仅提供通用范围算法，不声称已实现
canonical keyspace 快速路径。无持久化 I/O、全局状态或后台任务。

行为验收见 [批次 15 证据](../../../docs/rust/schedule/labeler/15-证据.md)。
