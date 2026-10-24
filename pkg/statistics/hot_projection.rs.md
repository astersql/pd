# 热点查询结果投影

对应Go：`store_hot_peers_infos.go`、`hot_regions_stat.go`、`store_load.go`。
[运行证据](../../docs/rust/state/task-25.md)固定真实Go返回值与JSON快照。

`collect_hot_peer_infos`按输入store列表收集，缺少peer map项时省略；存在空集合时保留。
`as_peer`包含所有peer，`as_leader`只包含leader；不按热度过滤。
只填逐维去噪取整负载之和与count，store rates为0，`statistics`为JSON null。

`get_hot_status`复用`summary_store_infos`/`summary_stores_load`，按leader与region两次投影，
保留批次24引擎、transfer限制、store loads存在性及方向维度规则。
没有pending/history输入，不读取墙钟或创建运行时。
`StoreLoadDetail::to_hot_peers_stat`只取current四维store rates（保留分数），
只输出HotDegree>0的peer，汇总与实际返回项一一对应。空详细列表为[]。
`HotPeerStatShow::from_peer`复用get_load的Go math.Round语义，输出owned stores。
输入领域Vec的空stores表示未设置，序列化为null；调用者可显式设置Some(empty Vec)输出[]。

JSON字段显式映射为Go契约：store_bytes/keys/query/cpu、total_flow_bytes/keys/query/cpu、
regions_count、statistics，逐项为store_id/stores/is_leader/is_learner/region_id/hot_degree、
flow_bytes/keys/query/cpu、anti_count、last_update_time。u64 ID不经f64转换。
learner与更新时间由上层提供，默认false及0001-01-01T00:00:00Z；
Go的time.Time即使带omitempty也输出该零时间。本模块不查集群，不引入HTTP/协议依赖。
调用者补充last_update_time时负责提供Go兼容RFC3339字符串，库不拥有时间来源。
所有结果独占，输入顺序保留在peer列表中，store map按ID排序。
