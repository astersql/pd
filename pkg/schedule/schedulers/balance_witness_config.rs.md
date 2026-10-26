# 见证均衡配置更新与恢复

生产入口位于 `BalanceWitnessScheduler`：`encode_witness_config`、
`update_witness_config`、`reload_witness_config`。复用 canonical
`BalanceWitnessConfig` 的 batch 1–10 校验与二进制 key 序列化；配置整体替换，
HTTP PATCH/状态码与请求解码由适配层负责，不在此新增。

持久化使用宿主的 `StorageEndpoint`、`ClusterPaths` 和固定 scheduler 名
`balance-witness-scheduler`，不选择持久化后端、不创建运行时。更新持有 Rust
独占可变借用，编码相同返回 false 且不访问存储；变化先保存再发布 batch/ranges，
返回 true。保存失败传播 `SchedulerError::Storage`，保留全部原活动配置。
恢复先加载和完整校验再发布；读取失败或非法 batch/key JSON 保留原配置，
分别传播 Storage/Decode。不同实例共享存储仍是 last-writer-wins，没有新增 CAS。
空 ranges 沿用前置配置类型的全域规范化，不改变其既有表示。

任务74的明确验收要求优先：Go `balance_witness.go:81-85` 保存失败仍发布新值并
返回成功，这里有意遵从任务的保留旧值契约，并不声称与该 Go 故障行为相同。
Go `ReloadConfig:209-219` 直接赋值；Rust 按任务要求使用校验后的配置类型，
拒绝非法 batch。Go update 的部分 JSON 合并、未知字段与错误中途字段修改行为
不由本领域整体替换API模拟。

按用户明确授权，验收采用只读Go源码/测试推导加Rust真实行为和故障验证。
没有本任务固定来源Go运行输出，不声称运行差分。Rust测试期望冻结于
[fixture](../../../docs/rust/schedule-execution/fixtures/task_074.json)，明确标注源码
推导和任务契约；用户已解除原Go运行oracle准入限制，当前验收证据完整。

[独立测试](task_074_test.rs) 使用真实 canonical core/placement/operator 和
MemoryStore，经 I/O trait 边界注入读取/事务不可用，核对活动配置、持久化值、
重试恢复以及配置范围对真实调度命令的影响；不以调用次数替代状态断言。
[验证证据](../../../docs/rust/schedule-execution/task-074.md) 记录命令、计数及限制。
