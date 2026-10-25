# 拆分散射观察状态

`SplitScatterObserver` 保存 source/children 的最新批次身份；
`observe_split_scatter_result` 从真实 `RegionsInfo` 读取结果及 source，返回
`SplitScatterObservation` 的类型化状态。没有协议、运行时、存储或外部 I/O 依赖。

结果版本必须等于记录的 split version；较低为等待，较高为 stale。
source 版本至少达到 split version 即可，因此 source 后续再次拆分不会阻止旧批次的
匹配 sibling。缺失结果和缺失 source 分别保留为 pending 状态，缺失 epoch 的版本为零。
就绪以单个结果为单位；source 已观察到且结果匹配后，不要求所有 siblings 同时存在。

`record_batch` 是观察身份登记入口：记录 source、所有 children，共享
`split-scatter-{source}-{first-child}` 分组；version 0 变为 1，非零请求版本不向
缓存中的较新版本提升。重复 ID 覆盖，空批次和 disabled 输入不记录。
调用者提供相同单调时钟上的 deadline，deadline 与 region ID、group、source ID、
split version 一起构成身份。`is_current` / `remove_if_current` 防止旧快照删除新批次。
同身份且同 deadline 的重复登记保持 Go 的相等语义，不添加隐含 generation。

`ready` 只返回观察就绪快照，按 ID 排列，不消耗 pending，不执行调度排序、退避、
TTL 回收、capacity 管理、operator 创建或 gauge 更新；这些策略属于后继任务54。
派发前调用者须再次读取 region/source 并校验 `is_current`，避免跨读取间隔使用旧状态。
观察 API 采用独占可变借用更新，宿主负责共享锁及跨组件调用的串行化。
任务54的重试控制器通过包内 `snapshot` 获取拥有身份的快照，通过包内 `clear`
清空同一 canonical 身份表；策略 metadata 与指标由重试控制器同步维护。

`has_split_scatter_txn_keyspace_bounds` 复用 canonical codec 的 keyspace boundary，
从真实 registry 验证两个精确 start key；仅包含某个 bound 或以它为 end key 不足以
建立 keyspace。作为任务52 range hint 的 validator 可防止把用户数据误判为 Txn keyspace。

来源是只读 `split_scatter.go` 的版本 gate、批次登记及身份检查，和 keyspace
`MakeRegionBound`。按用户授权，验收为 Go 源码/测试契约推导与 Rust 行为验证，
没有 Go 运行输出；完整来源哈希、fixture、失败和变异证据见
[任务53证据](../../../docs/rust/schedule-execution/task-053.md)。
