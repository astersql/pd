# 热点动态阈值

在同一 canonical statistics crate 为 `HotPeerCache` 接入真实 utils::TopN，
`update_stat` 的 Add/Update 保存去噪、四维独立拥有的 HotPeerStat 快照，Remove 删除对应
store/region，未知动作保持索引。窗口状态仍由原有直接报告状态机持有；快照避免堆内比较键
被后续窗口变动破坏。每个 store 独立 TopN，read/write 缓存实例隔离。

`calc_hot_thresholds(store, now)` 返回 byte/key/query/CPU 四维速率：未满60个元素使用
方向默认最小阈值；达到60后逐维取第60大 GetLoad * 0.8，与最小阈值取大。
GetLoad 使用真实 TimeMedian 的取整去噪负载，各维排名可不同。替换和删除通过真实堆重排。
默认 read 下限 [8192,128,128,1]，write 下限 [1024,32,32,1]。

时钟由宿主传入有符号纳秒，刷新缓存默认8秒。经过时间严格大于刷新间隔才重算，
等于间隔、相同时刻或时钟回退保留旧值，差值用i128避免溢出。
`set_thresholds_update_interval` 是每实例纳秒配置，0间隔在下个严格较晚时刻刷新。
动作应用不主动失效已计算阈值，与 Go 相同。snapshot互斥锁保持缓存可Send/Sync，宿主仍须
串行检查/应用报告；库不引入线程、运行时、全局时钟或后台任务。

`check_peer_flow_dynamic` 将真实TopN阈值接入原check_peer_flow，保留短报告降噪策略、
None/空peer区别及事务所有权。新peer按瞬时速率接纳，已有peer按完整窗口last average
（含CPU，inclusive阈值）升降温；阈值的去噪中值和判热的最近平均是两个不同量。
检查会缓存阈值，丢弃返回动作不改变peer窗口或TopN成员。
原check_peer_flow阈值闭包接口继续可用，便于外部提供已计算阈值。

TTL/GC、继承、leader转移及async队列保留后续边界，无新增依赖或协议/调度反向依赖。
测试在独立hot_threshold_test.rs显式注册；[验收证据](../../docs/rust/state/task-20.md)。
