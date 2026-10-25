# 共享调度规则与分配契约

`shared_cluster.rs` 对应 Go `cluster_informer.go` 的 `SharedCluster`。
`SharedClusterView` 借用宿主的 canonical BasicCluster、配置、statistics、placement、
labeler、affinity 和 keyrange；不建立第二套规则或 region 缓存。
BasicCluster 的 store/region 接口与 RegionStatInformer 直接转发到这些对象。

宿主在发布配置/规则 generation 的边界装配视图。共享借用阻止同时替换配置与
规则对象；更新仍通过真实管理器完成。keyrange 保留 canonical 内部锁，两个消费
视图可立即观察同一 owner 的修改。statistics 的借用要求 Sync，真实视图测试检查
Send + Sync。视图不持有后台任务、Tokio runtime、存储实现或 wire 类型。

`SharedIdAllocator` 和 `PrepareRegionCount` 是宿主服务边界，保留各自关联错误
类型及 async Future。`IdAllocation::Classic` 委托真实持久化 Allocator 的
`alloc_count(1)`，忽略输入 count，包括0；`Counted` 保留输入 count。
远端 RPC 分配器通过同一领域契约接入，不被要求创建本地分配器。共享层不重试
分配、不产生 ID，也不把错误改为零值成功。经典本地 prepare 查询直接读取
BasicCluster 的 region 总数；远端 HTTP 查询/timeout 属于后续宿主适配，测试只在
这个外部 I/O 边界注入已冻结的 Go 错误，并验证同一错误对象得到传播。

halt 为同一配置 generation 的 halt_scheduling 与宿主 recovery_running 的逻辑或。
微服务没有 classic unsafe recovery 时传 false。库不自行驱动恢复流程。

Go 运行 oracle 来自既有 Go 测试二进制，通过调试器调用真实管理器/分配器并读取
返回值；没有改写 Go 源码、断言或使用 failpoint 注入。ID exhaustion 使用真实
etcd；Rust 使用 canonical MetaStore MemoryStore，另外通过真实 lease 撤销验证
leader fencing。classic 请求计数映射和 recovery OR 明确标为源码契约检查；
不将这些检查宣称为独立的端到端服务差分。

详细 provenance、输入/输出、失败测试、故障、命令和计数见
[任务证据](../../../docs/rust/schedule-execution/task-003.md)，冻结数据见
[fixture](../../../docs/rust/schedule-execution/fixtures/task_003.json)。
