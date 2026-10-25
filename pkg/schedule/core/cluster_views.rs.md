# 调度器与检查器专用视图

来源：`cluster_informer.go` 的 `SchedulerCluster` / `CheckerCluster`，以及
`server/cluster/scheduling_controller.go::UpdateRegionsLabelLevelStats`。
Go 基准为 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；运行 oracle、fixture
与故障边界见[任务 004](../../../docs/rust/schedule-execution/task-004.md)。

`SchedulerCluster` 聚合共享观察、store loads、bucket FIFO 查询及 scheduler/store
配置；`CheckerCluster` 聚合共享观察、checker/store 配置和 label stats 更新。
两者通过 `StoreConfigCluster` 共用一个 store 配置 getter，消除聚合使用时的方法歧义。
`ClusterViews<'a, S>` 是实现这两个消费契约的宿主借用代际，`S: SharedCluster`
直接复用任务 003 的规则、分配、prepare、region stats 和 canonical BasicCluster。
领域层不依赖 server/runtime、protobuf 或具体 MetaStore 后端，没有新的状态缓存。

所有配置 getter 均借用 `S::Config`，专用 getter 不复制或重置配置。
沿用 canonical `ProviderSnapshot` 的发布纪律：宿主调整默认配置、捕获完整代际，
消费者持有不可变借用，宿主在释放借用后发布下一代并重新借用。这里不实现 Go 的
TTL、Setters 或持久化编排，也不声称持有旧快照时能观察未来配置。统计拥有自己的
同步机制；store 查询读真实 `StoreStatInformer`，bucket 查询向真实
`BucketWorkerClient` 提交 FIFO barrier。结果为独立 owned snapshot。

`update_regions_label_level_stats` 要求 `&mut self`，借用宿主已有的
`LabelStatistics`；不会建立第二份计数。它依次按 peer 查当前 store，忽略缺失
store，并按 core 的精确 engine 标签匹配排除 TiFlash。每个 region 用当前配置的
location labels 调用 canonical `observe`，最后无条件 `clear_defunct_regions`，
包括空 batch。overlap/删除发生时的 defunct 标记属于宿主 heartbeat 编排；本层
保留“先 observe，后 clear”的 Go 顺序，不能因为重复 observe 就取消删除标记。

store 尺寸检查保留 canonical `MergeAgainError`；共享 ID/prepare 关联错误直接透传。
Go 的 `BucketsStats` 返回 map，Rust 保留现有 worker 更明确的失败边界：
`BucketQueryError::Submit(BucketSubmitError)` 与
`BucketQueryError::Wait(BucketWaitError)`，并通过 `Error::source` 保留原错误。
这两个错误类别是已有 Rust worker 契约的组合，不冒称 Go 有等价错误返回。
取消一个查询只结束等待，不取消宿主 worker 或丢弃已接收报告；失败不返回部分统计。

测试单独注册在 `task_004_test.rs`。冻结 oracle 覆盖 store 更新/删除、bucket 的
完整字段和筛选、TiFlash 排除、defunct 清理、两代配置及真实 merge-again 错误。
额外 Rust 故障测试使用真实 worker 的取消和关闭，不使用 mock 调用计数或成功桩。
