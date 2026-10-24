# 配置持久化与重载

对应 `persist_options.go` 的 persistedConfig、Persist/persistLocked、Reload 和
adjustScheduleCfg；基准 `db1b56605b43b973afbd1eef8d03a7e7ba25e0bf`。

公开 `PersistedConfig { config: Config, store: StoreConfig }` 复用 canonical 值类型，
JSON 通过 flatten 保持 Config 顶层封装及注入的 `store`。`persist` 按 Go 仅拷贝
schedule、replication、pd-server、replication-mode、label-property、keyspace、
micro-service、cluster-version，其他配置以 Config 零值序列化。`reload` 保留宿主
静态配置，仅在完整加载、解码和 GC 并发验证成功后替换上述动态分区及 store。

读取通过现有 `StorageEndpoint::load_config<Value>` 保留持久字段 presence，然后把
字段覆盖到 Config.Adjust 的 reload 默认值。缺失/空字节返回 false，JSON null 是
存在的默认配置；struct null 保留预置值。保留 string bool、duration、nullable
collections 和 store 的原有 serde 编码，拒绝数组冒充 struct。默认 scheduler 按类型
补齐，保留已存在的禁用项和未知 scheduler。旧 store-balance-rate 按 presence 迁移，
显式零 rate 优先；旧 store entry 缺少 peer rate 为零，缺少 leader rate 取迁移默认。
废弃 disable flag 为 true 时清除对应 enable，清除 disable-raft-learner。
store.adjust 刷新派生缓存；候选成功后才将 store_limit 默认值反馈宿主。

`ConfigPersistenceError` 区分 endpoint 的有类型 I/O/codec 错误与配置验证错误。
无后台任务、全局状态、私有 runtime 或锁；&mut self 约束单宿主发布，未来热更新
编排负责并发序列化，不在此提供并发持久化锁或回退 API。Endpoint 的 proto 传递
依赖仅为已有适配器实现；公开配置和基础 schedule crate 不依赖协议类型。

独立测试显式登记于 lib.rs；真实 Go 二进制的保存 JSON 与重启证据、两类真实
MetaStore 以及断开 I/O 的恢复验证见 [任务 12 证据](../../docs/rust/schedule/config/12-证据.md)。
