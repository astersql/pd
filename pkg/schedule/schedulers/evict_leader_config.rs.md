# 领导者驱逐配置事务

[实现](evict_leader_config.rs) 扩展 canonical `EvictLeaderScheduler`，复用
`EvictLeaderConfig`、`get_key_ranges`、核心集群的引用计数暂停和
`StorageEndpoint`/MetaStore；没有新增状态副本、crate、feature 或运行时。
HTTP 路由、状态码和错误 JSON 留给适配层；领域 API 接受配置 JSON 值并返回类型化错误。

- `EvictLeaderConfig::from_args`：精确十进制 uint64；逗号多 store 不接受范围参数，
  重复 ID 合并，默认 batch 3。范围复用已有 URL 字节解码器，奇数尾参数忽略。
- `update_evict_config`：先完整校验 ID/batch/ranges，再暂停新增 store，单次保存候选配置，
  最后发布。省略或空 ranges 保留旧范围，新 store 默认全键空间；范围逐 store 独立拥有。
  `&mut self` 串行化本实例配置访问；等待 I/O 期间只读集群可能看见新增 store 的暂停，
  如同 Go 配置锁不覆盖整个集群。错误保留 batch、范围、持久化状态及其它暂停引用。
  取消尚未提交的 I/O 时 RAII 释放本次新增暂停；存储提交后的网络不确定性遵循后端契约，
  不宣称跨集群和 MetaStore 的分布式原子事务。
- `delete_evict_store`：保存删除候选成功后发布并恢复 transfer-in；最后一个 store
  调用宿主提供的移除回调。一般回调错误恢复内存及暂停，NotFound 不恢复。
  **与 Go 一致，回调失败时持久化配置仍为空**，不能将内存回滚解释为二次持久化。
  `None` 表示非最后 store，最后 store 成功返回固定提示。
- `reload_evict_config`：读取、解码后对 store 集差异暂停/恢复；保留 ID 不增加暂停引用。
  batch 缺失或零默认 3。新增 store 暂停失败仍发布，并把错误列表返回宿主记录。
  构造与解码继续采用前置任务的有效 batch 1–100 和非负整数 schema；损坏/不兼容
  存储配置拒绝并保留原值，不把 Go 未校验的任意 persisted int 视为有效领域输入。

`EvictConfigError` 区分参数、带位置的范围解码、调度序列化/存储、暂停、配置不存在
及移除回调错误；回调 NotFound 有独立类型。Go float64 的 `float64(MaxUint64)`
舍入成 2^64，且超范围浮点转 uint64 的结果未由语言规范固定。本实现拒绝该不可表示
边界，防止 Rust 饱和转换悄然产生最大 ID；精确最大 uint64 可通过创建字符串传入。

期望来自只读 `evict_leader.go`、`init.go`、`utils.go` 及相邻测试的源码推导，
按用户计划级授权替代缺失的 Go 运行 oracle；没有 Go 运行差分声明。
[固定案例](../../../docs/rust/schedule-execution/fixtures/task_076.json) 与
[验证证据](../../../docs/rust/schedule-execution/task-076.md) 包含来源、范围限制、
暂停中途失败、真实 MemoryStore I/O 失败/取消、删除回调、reload 和反例测试。
