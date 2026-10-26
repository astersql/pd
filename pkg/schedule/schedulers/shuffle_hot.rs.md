# 热点随机扰动调度

`shuffle_hot.rs` 是 `shuffle_hot_region.go` 的 canonical Rust 领域实现。
`ShuffleHotScheduler::schedule` 接收宿主本轮准备的真实 `StoreLoadDetail` 四类快照，
按 Go resourceType 顺序抽样；只对 readLeader/writeLeader 产生操作，peer 资源返回空。
宿主负责统计刷新、计数、提交、工厂注册、HTTP 和 metrics，领域库不创建 runtime。

每个详情随机取一个 hot peer；不存在的区域或 down/pending 区域跳过整个详情。
目标由真实 StoreStateFilter（MoveRegion、Low）、HotRegionEvictedTargetFilter、
peer 排除和 PlacementGuardContext::safeguard 过滤。按 ID 排序候选消除 map 遍历
非确定性，再使用显式随机输入；这保留合法候选集合与随机分布，不声称 Go map 顺序相同。
详情切片顺序是宿主提供的 Go map 遍历边界，不按负载排序。抽样 peer_word 对每个
详情使用同一输入，宿主拥有每轮随机源。

当前区域 leader 是迁出源，即使抽到的统计 peer 属于 follower。没有合法目标时立即
结束本轮，不尝试其他详情。通过真实 PeerOperatorFactory::create_move_leader 构造
REGION|LEADER、Low、description=random-move-hot-leader 的操作，搬迁旧 leader peer
并将 leader 移到新 peer；不增加 HOT_REGION kind。Builder 错误保留类型供宿主诊断。
缺 leader/source 的异常状态返回 Source，不沿用 Go nil 解引用风险。

`is_schedule_allowed` 对 hot/region/leader 三类计数分别严格小于对应限额。
`ShuffleHotConfig` 构造默认 limit=1，slice 解码只有恰一参数才解析，否则保持1。
`update` 接受已解析的 typed config，通过真实 MetaStore endpoint 先持久化再发布，
写失败保留原值，与 Go handler 回滚后的可观察结果一致；JSON 浮点转 uint64 由 HTTP
适配层负责。reload 读取/解码失败保留旧值，空对象 limit=0 与新 Go config 的零值一致。

验证采用用户授权的只读 Go 源码/测试推导 + Rust 真实行为和故障验证，未取得、也不
声称 Go 运行 oracle。来源、命令、有效案例和故障结果见
[任务证据](../../../docs/rust/schedule-execution/task-100.md)。独立 task_100_test.rs
显式注册，正常后端为 MemoryStore，失败仅注入 allocation / MetaStore I/O 边界。
