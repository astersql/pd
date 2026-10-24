# 单副本热点窗口

[hot_peer.rs](hot_peer.rs) 在 canonical statistics crate 提供 `DimStat` 和
`HotPeerStat`，复用真实 `TimeMedian`、`ActionType`、`RWType` 和 `TopNItem`。
不引入新依赖、protobuf、调度决策、全局时钟、锁或runtime；共享时由宿主同步。

`DimStat::add` 接收 delta 和 signed 纳秒 interval；TimeMedian 使用1个报告窗口与5点中值。
last-average 累计每次 interval 截断后的整秒和 delta，独立于中值；CPU 与其他维度同样判断。
`is_full` 比较整秒，`clear_last_average` 保留中值。
`DimStat::clone` 深拷贝真实窗口及累计秒数，并按Go的特殊契约重置lastDelta为0。
零分母遵循IEEE：0/0不热，正delta/0为正无穷；不额外替Go过滤负数/NaN。

`get_load` 对byte/key/query/CPU四维优先读取rolling并round（半值离零），缺失则回退Loads，
越界返回0。`get_loads` 在rolling为None时返回原始长度和数值的独立副本；Some即固定4维去噪。
`HotPeerStat::clone` 是Go Clone的快照：四维去噪Loads、无rolling，其余元数据和stores独立复制。
因此snapshot Clone和DimStat Clone有不同语义。`is_hot`跳过nil维并短路，访问到的非nil维必须有threshold，
否则与Go一样panic。公开字段便于后续缓存任务装配，但本任务不实现缓存生命周期。

冷却使用宿主提供的now/last_transfer_leader_time纳秒，严格小于minHotDegree乘报告秒数，
READ为10秒、WRITE为60秒；None映射未转leader，未来时间仍冷却。调用方需在同一时钟域提供时间。
`interval_sum`使用第一维，缺失返回0；TopN按region ID与去噪负载排序，保留时不得修改比较值。

独立[行为测试](hot_peer_test.rs)显式登记，oracle和红绿验收见[任务18证据](../../docs/rust/state/task-18.md)。

内部copy_for_report保留pending delta与时间窗口，供直接报告事务检查；
继承Clone仍按Go重置pending delta，两种复制语义分开。
