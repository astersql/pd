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
PersistedConfig 值模型无后台任务、全局状态、私有 runtime 或锁；&mut self 约束
单宿主 reload，HotConfig 负责运行期并发序列化、持久化锁和回退。Endpoint 的 proto 传递
依赖仅为已有适配器实现；公开配置和基础 schedule crate 不依赖协议类型。

独立测试显式登记于 lib.rs；真实 Go 二进制的保存 JSON 与重启证据、两类真实
MetaStore 以及断开 I/O 的恢复验证见 [任务 12 证据](../../docs/rust/schedule/config/12-证据.md)。

## 热更新发布与回退

`HotConfig::{new,snapshot,update_schedule,mutate_schedule,persist}` 在宿主层封装
上述值模型与既有 endpoint。所有写者共用一个异步 mutex；读者通过短暂 RwLock
捕获 `Arc<PersistedConfig>`，不在存储 await 时持读锁。已捕获的 generation 不会
被后续发布或回退修改；调用方可以从同一快照借用 schedule/replication/store，
供既有 provider 组装读取视图。

`update_schedule` 在写锁内读取最新值并 Clone，callback 的 current 只读、next
独立；返回 false 或错误时不发布、不保存。成功变更先发布 next，再保存完整动态
配置。保存错误恢复原 Arc，读者在保存期间可以观察候选值，之后重新捕获则得到
旧值，与 Go UpdateScheduleConfig 的顺序相同。存储成功时保留候选值。
`mutate_schedule` 对应 Go mutateScheduleConfig，仅修改内存；`persist` 与前两者
共用锁，避免整个配置保存与 schedule RMW 相互覆盖。底层 PersistedConfig 仍是
离线值模型；运行中的宿主须通过 HotConfig 持久化，不能另造未共享锁的写者。

取消 pending future 时，私有 Drop guard 先恢复旧值，再释放写锁；不启动后台任务
也不安装私有 runtime。后端 Unavailable 可能意味着写入已发生，回退只保证内存
顺序，不能撤销不确定的远端提交，且不会盲目重试。callback 负责它所需的领域校验，
与 Go 原始 UpdateScheduleConfig 一样，不隐式调用服务 API 的 Validate/Deprecated。
TTL、SwitchRaftV2 及其它配置分区的运行时更新不属于此入口。

[任务 13 证据](../../docs/rust/schedule/config/13-证据.md)记录真实 Go HTTP 并发与
NOSPACE 回退、只读调试得到的发布时序，以及真实内存/etcd 和 I/O 边界故障测试。
