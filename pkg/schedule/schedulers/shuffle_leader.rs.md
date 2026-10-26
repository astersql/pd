# shuffle-leader 领域调度

`ShuffleLeaderConfig::{new,from_args,ranges}` 保留 Go 的 ranges JSON、百分号参数解码和
空参数全范围语义；缺省/null 编码为 null，显式空数组编码为 []，两者采样均覆盖整树。
`ShuffleLeaderScheduler::{new,config,encode_config,is_schedule_allowed,schedule}` 使用现有
canonical core/filter/operator，不建立第二份集群状态或私有运行时。

来源为相邻 `shuffle_leader.go`、`init.go`、`scheduler_test.go::TestShuffleLeader` 和
`pkg/core/region_tree.go::RandomRegions`。采用用户授权的只读 Go 源码/测试推导及真实
Rust 验证；没有 Go 运行 oracle 或运行差分，详见
[任务证据](../../../docs/rust/schedule-execution/task-101.md)。

宿主通过 `EvictLeaderContext` 提供真实 BasicCluster、配置和 RuleManager，以及显式
时间、合法有界随机索引和 range permutation。store ID 排序后随机选择，归一化 Go
未规定的 map 顺序。先按 Low 级 StoreStateFilter 与 SpecialUseFilter 选目标，再从其
follower 索引按 ranges 抽取区域，拒绝任何 pending/down peer。不重试其它目标；learner
不在 follower 索引，witness 或 placement 不允许的 leader 由真实 builder 拒绝。

生成单个 ADMIN|LEADER 操作，优先级 Low，空替代目标列表，无 peer 分配、无 confver
变化。真实操作器负责后续命令和完成状态。Go 日志吞掉的 builder 错误在返回结果中保留
类型用于宿主诊断；无可行目标/区域正常返回空操作。限额单独使用严格 `count < limit`，
由协调宿主在调用前执行，与 Go 分离的 IsScheduleAllowed 一致。

配置持久化使用已有 `SchedulerConfig<ShuffleLeaderConfig>` / StorageEndpoint；测试
以 MemoryStore 及其 read/transact I/O 故障验证保存、恢复和失败保留旧值。Go 此调度器
尚无专用 ReloadConfig，本模块也不新增 HTTP、热更新或工厂扩展协议。

独立 `task_101_test.rs` 验证24个场景×两种心跳模式、配置/限额及真实存储故障；逐项
断言目标、kind、优先级、步骤、影响量、confver、命令及应用 leader 后完成状态。
