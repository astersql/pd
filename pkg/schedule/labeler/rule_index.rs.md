# 通用标签范围索引

`LabelRuleIndex` 复用 canonical `LabelRule` 和 `rangelist::Builder/List`。
领域接口不依赖协议、存储或 runtime；输入规则须预先 `check_and_adjust`。

公开 API：`new`/`default`、`rule`、`rules`、`set`、`delete`、
`get_split_keys`、`get_range_rules`、`min_expire`、`remove_expired_labels`。

索引以 Arc 拥有权威规则，禁止通过引用改写派生数据。规范键空间规则进入稀疏
KeyspaceRuleIndex，其余规则进入真实 rangelist。set/delete 在返回前发布视图，
规范更新仅修改所属槽；通用分区变化才重建范围与最早过期时刻，相当于 Go
写锁内修改后 Unlock 发布的快照。重复 ID 后者覆盖；
不存在的删除为 no-op。空 end 表示无穷，split keys 严格位于查询区间内部。
跨越任一边界返回 None，无规则但未跨边界返回 Some(empty)。同一规则的重叠
范围保留多次出现，与 Go builder 一致。返回规则次序无业务含义，index 较大者
优先；同优先级冲突的区域标签选择留给批次 19，不增加 ID 决胜规则。

过期过滤使用调用者时间，边界相等仍有效；过滤不删除规则，清理由调用方决定。
键空间专用分区/稀疏加速见 [keyspace_index.rs.md](keyspace_index.rs.md)。合并查询
保留既有 Vec<&LabelRule> 接口，将通用规则和单个键空间规则一起返回；任一分区
存在内部边界均返回 None，split keys 按字节序合并去重。过期扫描在确有到期标签
时才用 Arc::make_mut，避免无变化扫描破坏键空间对象身份。无持久化 I/O、
全局状态或后台任务。

行为验收见 [批次 15 证据](../../../docs/rust/schedule/labeler/15-证据.md)
和 [批次 16 证据](../../../docs/rust/schedule/labeler/16-证据.md)。
