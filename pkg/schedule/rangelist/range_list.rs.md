# List 查询

`get_data_by_key`、`get_data` 返回 `(isize, Option<&[T]>)`，缺失索引为 -1。
`None` 表示没有段，`Some(&[])` 表示存在空数据段。只读借用保持 Builder 快照完整。
`get_data_by_range` 返回起点的数据及区间内无分裂点标记；位于首段前且不跨断点
时为 `(None, true)`。`get_split_keys` 返回严格落在 `(start, end)` 的借用键。

四个 API 共用 O(log n) 上界搜索；分裂点收集额外 O(k) 时间和空间。
空 end 表示正无穷，end 等于断点时不跨段。保留 Go 对空长度和倒置范围的
原始比较语义，不引入额外合法性校验。无 I/O、时钟、协议类型、后台任务或依赖。

对应 Go range_list.go；原 Go TestRangeList/TestRangeList2 及独立
range_queries_test.rs 的覆盖与运行证据见 docs/rust/schedule/rangelist/5-证据.md。
