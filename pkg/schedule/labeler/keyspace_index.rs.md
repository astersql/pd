# 键空间标签索引

Canonical crate：`astersql-pd-schedule-labeler`，由 [lib.rs](lib.rs) 导出。
Go 参考：[keyspace_index.go](keyspace_index.go)、[keyspace_index_test.go](keyspace_index_test.go)。
运行差分与测试证据：[批次 16](../../../docs/rust/schedule/labeler/16-证据.md)。

公开符号：`MAX_KEYSPACE_ID`、`parse_keyspace_rule_id`、`merge_split_keys`，以及
`KeyspaceRuleIndex::{default,is_empty,add,replace,remove,contains,get_rule,
match_rule,has_split_key,get_split_keys}`。

只有 `keyspaces/<规范十进制 24 位 ID>`、index=0、单个永久 id 标签（value 与
ID 原文相同）、每种 API mode 至多一个精确范围的规则进入专用索引。
TTL、start_at、派生 expire、重复 mode、非规范边界和其他标签均回退通用索引。
所有边界编码与完整前缀解码复用 canonical codec，不复制协议类型。

Rust 使用 `BTreeMap<(mode,id), Slot>` 保存实际存在的稀疏槽，空间与已分配槽数
成正比，不分配 24 位全集。查找、增删为 O(log n)，边界扫描为 O(log n + k)，
直接从查询边界计算 ID 上下界，避免扫描空洞；这与 Go 的 chunk/bitmap 内部布局
不同。未承诺 Go benchmark 的具体吞吐或百万规则启动时限。每槽保存两个 9 字节
边界，未建立通用 rangelist segment；相邻边界去重。两 mode 分别按编码顺序返回。
规范形状校验用栈数组，不分配 Vec 或 ID 字符串；同形状 replace 复用槽，仅更新
Arc，未建立新树节点。mode 变化可插入/删除节点。

Arc 的对象身份对应 Go 指针归属；相同 ID/字段的不同 Arc 不拥有原槽。
Add/Replace 先检查全部槽，再原子更新；Replace 只在新 ID 已有 old 归属时成功，
不能跨 ID 搬移。Remove 按传入规范 ID 和归属判断，不依赖旧规则仍保持规范形状。
外部持有 Arc 时 Rust 保证不可就地改写共享规则；不会暴露失配的可变派生状态。

match_rule 区分“单个 keyspace 内没有规则”和“跨 keyspace”；完整解码保留 Go
同四字节前缀快速路径，非完整 marker/短前缀则以字节序边界定位。最大 ID 的
右端使用 mode+1 fence，空 end 为无穷。split keys 严格排除两端；HasSplitKey
短路且不分配返回集合。merge 接收已排序列表，双非空时合并去重；一侧为空
直接返回另一侧，与 Go 相同。

没有存储、I/O、指标、后台任务、全局状态或 runtime。独立测试位于
[label_keyspace_test.rs](label_keyspace_test.rs)，由 lib.rs 显式登记。
