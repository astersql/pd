# 热点调度配置领域契约

`hot_config.rs` 是 `astersql-pd-schedule-schedulers` 的 canonical 配置实现。
`HotRegionParams` 提供当前有效参数、默认值、校验、rank/forbid 投影和 zombie duration；
`adjust_hot_priorities` 按 read/write-leader/write-peer 和 CPU/query 支持选择维度。
`HotRegionConfig` 持有原始参数及不持久化的支持标志，提供 `new`、`params`、
`set_support`、`valid_config`、`encode`、`update`、`update_json`、`reload`。
`HotConfigUpdate` 区分无变化与已发布更新，并保留实际持久化错误供宿主报告。

领域不依赖 HTTP、protobuf、server/runtime，不创建 Tokio runtime。
宿主将已有集群版本检测结果传给 `set_support(query, cpu)`；本任务不移植 versioninfo。
配置 getter 是 owned snapshot，不借用可被更新的向量。`&mut self` 跨越发布及存储 I/O，
宿主按需加锁。base scheduler 的 disabled 管理沿用既有契约，hot `reload` 不修改该标志；
本对象编码包含默认 false 的 disabled 字段，未新增 disabled 管理入口。

## 来源和语义

只读来源为 `hot_region_config.go`、`hot_region_config_test.go`、`hot_region.go`、
`config.go`。Go 测试 `TestAdjustPrioritiesConfigCPUFallback` 的5例只涵盖 read 回退；
其余维度、校验、保存故障、恢复的期望来自相应生产分支。
依整个计划的后续用户授权，用源码/测试推导和 Rust 真实行为/故障验证替代缺失的
Go 运行 oracle；**没有执行该 Go 测试，没有 Go 运行差分**。
详细哈希、案例与运行结果见 [task-085](../../../docs/rust/schedule-execution/task-085.md)。

- 默认 read 为 cpu/byte，write-leader 为 query/byte，write-peer 为 byte/key。
  缺 query 时含 cpu/query 的列表先退回兼容列表；缺 CPU 时退回 query 列表。
  非两维、重复、未知维度在 effective 投影回退，原始值不变。
- 校验接受空列表或至少两种互异维度；write-leader 禁 CPU，write-peer 禁 CPU/query。
  三/四维 read 虽能通过校验，有效投影仍回退到两维。这是 Go 的两个不同契约。
- rank 接受空、v1、v2，空投影为 v1；forbid 接受空、none、read、write，前两者投影为空。
  pending-weight 至少1，split-thresholds 在闭区间[0.01,1]。Go 未校验其它阈值、
  比率、history interval/duration 的正值或大小关系，Rust 不擅自添加这些约束。
- 默认 history duration/interval 复用 canonical statistics 的300s/30s；带符号 duration
  复用 typeutil 的 Go 文本编码。strict-picking-store 与 enable-for-tiflash 使用字符串布尔。
  TiFlash tolerance 修正公开为0.1；solver 的应用仍归后续热点调度任务。
- `update_json` 以当前参数为底覆盖，拒绝非法类型/JSON/校验值；未知字段随实际更新忽略，
  仅未知字段且未变化时返回错误；已知无变化不写存储。scalar null 不改原值，
  priorities null 规范化为空向量（有效维度与空列表相同，重编码使用[]）。
- `update` 验证后先发布，再经真实 `StorageEndpoint` 保存；写失败不回滚，返回 warning。
  `reload` 从当前参数副本叠加存储后校验，成功才替换；读取/解码/校验失败保留原值。
  不加入历史迁移分支，原始优先级与 rank 不被 effective 查询改写。
- Rust typed update 拒绝不可 JSON 表示的 NaN/Infinity；这不扩展 JSON 参数的有效范围。
  JSON 输出比较按字段、数值、数组顺序、duration 文本进行；对象键顺序和空优先级表示
  不作为调度行为差异，其它参数、错误与状态都不忽略。
- 异步保存取消时，新值已发布，旧存储仍在；后续 reload 恢复旧值。
  这是 Rust 宿主取消边界的资源保证，不伪称 Go 取消差分。

## 验证

独立注册 `task_085_test.rs`，冻结96组优先级输入/输出及全部默认字段，另测11类拒绝、
24组 rank/forbid/threshold 边界、可变维度、阈值/history、TiFlash、真实 MemoryStore
保存/重建/恢复、12组坏更新、4组坏重载、读取/写入失败和实际 pending 事务取消。
测试后端只在 MetaStore read/transact 边界故障，正常操作委派 canonical MemoryStore。
先记录缺 API 红灯，再临时改错 CPU 回退、观察同一行为断言失败，恢复源码后验证通过。
