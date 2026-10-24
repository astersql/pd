# 热点副本升温与降温

`HotPeerCache` 是同步、单方向的直接报告状态机，复用
[HotPeerStat/DimStat](hot_peer.rs.md) 和真实 TimeMedian。
`check_peer_flow` 消费 canonical core RegionInfo、显式 store ID 列表（None 回退全部
region peers，空列表不处理）、8维累计 delta 和整数秒，阈值闭包返回各 store 的4维阈值。
`minimum_thresholds` 提供默认下限；动态 TopN 阈值见[热点动态阈值](hot_threshold.rs.md)。

默认丢弃小于3秒报告；`set_denoising` 为每实例策略，不引入全局开关。
首次按瞬时任一维 inclusive 阈值接纳，满报告窗口初始化 degree=1、anti=12(read)/2(write)，
否则0/0。后续完整窗口任一维last average达到阈值则degree+1、anti最多恢复1直到默认值；
冷窗口degree/anti各减1，anti<=0产生Remove。未满窗口保持二者；满窗清last average但保留median。
CPU和byte/key/query参与同一种判定，读写通过utils映射互不污染。

`update_stat` 消费动作所有权，Add/Update替换唯一(region,store)，Remove删除，未知动作忽略。
宿主须按事件顺序检查并应用结果。检查时复制真实窗口和pending delta，因此丢弃动作不会
推进已存状态；这是Rust所有权下的事务接口。它不同于继承用DimStat::clone（重置pending delta）
和HotPeerStat::clone（只返回去噪快照）。没有返回值借用或可变别名。

跨副本继承和leader转移见[继承策略](hot_inherit.rs.md)。
`check_peer_flow_at`接收宿主纳秒时钟；原`check_peer_flow`使用固定时钟原点0，实时宿主应使用前者。
TTL/GC和未上报peer扫描由hot_expire模块扩展；宿主提供排除tombstone的有效store集合。async队列留后续批次。
库不依赖协议、传输、MetaStore或调度，且不创建运行时、线程或后台任务。
独立hot_peer_update_test在lib.rs显式注册；[验收证据](../../docs/rust/state/task-19.md)。

[过期策略](hot_expire.rs.md)扩展update_stat_at，Add/Update刷新宿主纳秒时间，
Remove同时删除过期记录。update_stat保留时钟原点0的兼容入口；实时宿主应一致使用update_stat_at。
缺失报告复用update_hot_peer_stat，满窗口in_cold强制降温，不触发leader切换。
