# 存储心跳状态更新

[store_heartbeat.rs](store_heartbeat.rs) 的 `handle_store_heartbeat` 接受独占 BasicCluster、
可选宿主 HotStat、原始 StoreStats 和 Unix 纳秒时钟。独占借用使成员/region修改与报告串行。
未知store返回类型化StoreNotFound，文本保留 `store N not found`；不注册store。
Removed store成功且不更新raw stats、时间或热点；组合统计时仍清理不健康rolling条目。
其他store克隆core元数据、共享raw stats/smoothing holder，替换整份report并更新时间。
旧元数据快照的lastHeartbeat保持原值，raw stats按Go SetStoreStats共享语义可见。

statistics=None选择独立调度服务路径；Some时复用StoresStats观察和健康清理、SlowStat状态。
报告interval使用end-start的uint64 wrapping，缺失为0，单位为秒。所有peer先进入reported集合；
找不到region或该store peer时跳过read任务。query复用core的get+scan+coprocessor wrapping求和；
CPU复用region_read_cpu_usage，unified_read乘秒作为累积输入；write维全零。
peer_loads集中构造8维输入，原始值不圆整。HotCache真实算法执行速率、窗口和冷热转换；
异步快照沿用既有HotPeerStat::clone去噪圆整契约，测试另查真实缓存raw小数速率。
同一FIFO先Read后Cold，Cold保留报告过但未找到region/peer的ID。

热点任务为与Go一致的best-effort异步admission，满队列/已关闭不撤销core更新。
宿主可通过HotStat快照barrier等待已接纳任务；须显式shutdown以取消并join。
该函数不创建runtime或后台任务；不处理持久化、store limiter配置、唤醒及调度反馈。
返回NodeState供适配层构造活动store响应；Removed no-op的返回值只供领域调用者参考，
不能据此声称Go tombstone响应已赋State。

独立[测试](store_heartbeat_test.rs)覆盖原Go冷热序列、CPU/query/过滤、时间和raw字段更新、
Removed no-op、缺失interval/倒序interval、独立服务、已关闭队列和reported集合语义。
运行来源、红绿证据与基线见[任务41证据](../../docs/rust/state/task-41.md)。
