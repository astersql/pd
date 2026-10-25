# 异常副本修复

`ReplicaFaultChecker::check` 只覆盖 `replica_checker.go` 的 down/offline 两阶段。
复用任务39 `PauseController`、任务40 `ReplicaStrategy::select_store_to_fix`，以及
真实 `PeerOperatorFactory`。数量补齐、普通冗余删除及位置优化留给后续任务。

宿主提供 `ReplicaFaultContext`：typed checker config、canonical `StoresInfo`、
已选出的 placement rules、纳秒时间和 joint-consensus 版本能力；ID 分配使用可失败的
闭包。库没有全局状态、私有 runtime、协议类型或存储副本。宿主应在调用期间提供
一致配置/状态视图；返回值不修改输入 region。

先检查暂停；down 只比较 store 心跳经过时间，忽略 down_seconds，等于阈值触发。
缺失 store 停止该阶段；首个满足条件的 peer 尝试修复后也停止该阶段。没有 down
操作时再尝试 offline；存在任意 learner 时跳过整个 offline 阶段。canonical
RegionInfo 用非可空 DownPeer 表达已解码有效 peer，Go 的 nil peer 不进入领域输入。

voter 数量大于 max-replicas 时直接删除故障副本，不需要候选目标或 ID。否则使用
真实策略选择修复位置，再由工厂构建 move-peer；修复允许位置退化，但保留隔离及
目标健康过滤。成功操作的优先级为 HIGH。工厂失败返回 typed `BuilderError`，
操作为空；这保留 Go 的空操作结果，同时让宿主记录错误。`pending=true` 表示至少
一个策略因临时目标状态拒绝修复，宿主需将 region 放入既有 pending cache；永久
无解不新增 pending。该布尔值是待执行缓存写入意图，不声称库已经写入宿主缓存。

本路径没有 MetaStore/I/O 调用；故障边界为 ID allocator，以及真实 busy/removed/
missing store 状态。暂停沿用任务39的显式时间边界。启用 placement/joint 时沿用
工厂已验证的规则拟合与规划，本任务 fixture 固定 classic 非 joint 模式以逐步
验收异常副本选择行为。

Go 源码与相邻测试均只读。依用户授权，以源码推导冻结期望，不宣称运行 Go 或运行
差分；来源哈希和32个输入/期望见
[fixture](../../../docs/rust/schedule-execution/fixtures/task_041.json)。
测试独立注册在 `task_041_test.rs`，比较描述、kind、优先级、epoch、步骤、错误及
pending 意图，再逐个模拟真实心跳验证下一条操作和最终 SUCCESS。失败识别、全量
验证与槽位记录见[证据](../../../docs/rust/schedule-execution/task-041.md)。
