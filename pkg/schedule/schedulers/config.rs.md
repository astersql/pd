# 调度配置持久化

`config.rs` 是 canonical 配置实现，复用 `StorageEndpoint`、`ClusterPaths` 和宿主
提供的 backend-neutral MetaStore；没有协议类型、私有 runtime 或全局状态。

`SchedulerConfig<T>` 提供 `get`、`save`、`reload`、`update` 与 `update_json`。
候选值先通过既有 Go 风格 JSON codec 编码，再保存单个 scheduler key，成功后
同步替换本地值。编码、解码、读写错误保留原类型为 SchedulerError；更新失败不
发布候选值。`&mut self` 保证单实例排他更新，返回的共享引用不能与更新并存。
load 先完整解码再替换，因此坏 JSON 不会部分修改配置。缺失名称/空值返回 Decode，
与 Go `TestSchedulerConfig` 一致，不自动创建未知配置。`new` 提供调用者初始值，
不会自动执行 I/O；恢复需要显式 `reload`。

`update_json` 是完整替换，不是 Go 某些 HTTP handler 的部分字段合并。每个具体
调度器的 schema 负责字段、默认值与业务校验；未知字段需要 schema 标注
`serde(deny_unknown_fields)`。本任务用严格 Params schema 验证该错误路径，未声称
已经移植各策略的参数验证。没有注册新的调度器、HTTP API 或配置迁移格式。

这项原子发布契约来自任务66的明确要求，不能宣称 Go 全部路径都有同样语义：
`config.go` 的 base save/load 只做编解码与存储；`evict_leader.go` 的 applyStoreIDs
在保存失败后回滚；`balance_leader.go` 的 update 保存失败仍保留新参数。
`DefaultSchedulerConfig` 从 base.rs 原样搬到本文件，保留任务65与 Go config.go
的特殊兼容行为：set_disabled 先改本地值再保存，失败不回滚；is_disabled 重新读取，
失败时保留本地状态，只刷新 disabled、不载入其它字段。它不是新的原子更新 API。

没有跨实例 CAS、历史数据迁移或生产本地持久化保证。MemoryStore 是已有真实测试
后端；故障仅注入 MetaStore I/O。取消一个尚未提交的写入不会发布候选值。
远端若已提交但响应丢失，错误/取消后存储可能已改变；不得盲目重试，应 reload
对账。此限制继承 MetaStore 契约，不宣称内存与远端存储具备分布式事务。

Go依据为只读源码/测试推导，未运行Go、未伪造运行oracle；用户已解除运行准入。
实际红绿测试与完整验证见 [task-066](../../../docs/rust/schedule-execution/task-066.md)。
