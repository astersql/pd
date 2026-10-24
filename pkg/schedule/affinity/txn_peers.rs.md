# 分组副本事务与观察

`txn_peers.rs` 扩展 canonical `Manager`，不创建 runtime 或后台任务。

- `GroupObservation`：拥有的 `GroupState` 和 `affinity_version` 快照。
- `Manager::group_observation(id, now)`：读取分组及版本；缺失返回 `None`。
- `Manager::update_affinity_group_peers`：管理操作，版本0强制更新。
- `Manager::update_affinity_group_peers_with_version`：先校验输入，再检查版本和
  voter集合；陈旧或非过期组换voter时成功返回当前快照，不写存储。
- `Manager::observe_available_region`：复用真实 `core::RegionInfo` voter分类，
  空快照/已可亲和调度快照忽略；其余使用观察版本，吞掉更新错误，与Go一致。

宿主传入 StorageEndpoint、ClusterPaths、StoreInfo 快照及 Unix 秒时钟。
`&mut Manager` 将一次验证、持久化和发布串行化；宿主在共享访问场景中负责锁。
store存在检查按排序voter顺序，健康错误按原输入顺序；输出voter排序但不改调用方。
成功事务后发布layout、设置available、清affinity计数并递增版本，保留region计数。
单事务失败不会修改内存或持久group；使用既有 MetaStore 边界故障注入测试。
领域类型不依赖protobuf。源码、红绿灯与两个真实后端证据见
[37证据](../../../docs/rust/schedule/affinity/37-证据.md)。
