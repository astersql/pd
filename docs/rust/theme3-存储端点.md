# 存储端点与序列化清单（任务 1，批次 20）

日期：2026-10-23（Asia/Shanghai）。状态：已完成文档盘点。
审计 HEAD：`08db5fc3d7c379916595ab222559b780d4d4d007`；规划基准：
`5d5948052a9ee9e3fa7a09145fe2916a2912c223`。
本轮读取源码、已有测试断言及装配；没有运行 Go/Cargo、生成 oracle 或改变运行行为。
下文行为是源码推导，测试名是已有静态断言入口，均不等同于本轮运行差分。

## 准入与审查边界

批次 19 的提交 `08db5fc3d` 删除前序任务 12 并新增
[存储键值证据](theme3-存储键值.md)任务 12 节：KV 整包 40 passed / 0 failed /
0 ignored；workspace 499 passed / 0 failed / 1 既有 ignored；metadata、fmt、check
与 diff 检查退出 0。已核验该提交、证据及 `pkg/storage/kv/lib.rs` 注册实物，准入满足。
这些为前序历史证据，本轮不重跑、不当作 endpoint 已有测试覆盖。

已读根 `AGENTS.md`、`.github/copilot-instructions.md`、[ADR-001](architecture.md)、
[依赖基线](dependencies.md)、[移植指南](porting-guide.md)、只读计划及来源主题任务 3。
`rg --files -g AGENTS.md pkg docs .plans` 未发现更深指令。来源主题及 plan.md 不修改。
所有下文路径均相对仓库；后续任务号仅指定责任，不表示实现或运行验证已完成。
`pkg/storage/endpoint/Cargo.toml`、`lib.rs` 及 endpoint Rust 模块均为**拟建**。

## 行为、key/value、事务与缺口归属

记 C 为 cluster ID，N 为 NullKeyspaceID=4294967295。key 的真实构造以
`pkg/utils/keypath/absolute_key_path.go` 及同包各 helper 为准；本文逐文件附录列出精确
helper 名，保留前导斜线、十进制补零及无 cluster 前缀的 TiDB/resource_group 键。
所有 endpoint 写入均不自行附加 lease；保存的过期时间字段不等于 etcd lease。

| 文件/行为（公开符号见附录） | key 与 value、边界（源码推导） | 后续责任、已有断言入口 |
| --- | --- | --- |
| endpoint.go / util.go 编码与构造 | StorageEndpoint 嵌入 kv.Base，保存加密 manager 与恢复游标；loadProto 空值为 false，坏 protobuf 为 false+ErrProtoUnmarshal；JSON 包装 ErrJSONMarshal/Unmarshal。prefix 扫描每页 100，去前缀，末 key 加 NUL 续页；JSONByPrefix 为单次范围读而非该分页器 | 2；无直接 helper 单测，具体业务测试复用；不可统一所有方法的 bool 语义 |
| RunInTxnWithConditions | 空条件回退 RunInTxn，非空需要 ConditionalTxnRunner，否则错误；同一条件事务，Go clientv3.Cmp 不得泄漏到 Rust 领域 | 2 最小适配，完整条件转接归存储门面/选主子计划 |
| cluster_id.go | /pd/cluster_id，8 字节 uint64 大端；先缓存/读，缺失生成 Unix 秒左移 32 加随机 uint32。CreateRevision=0 Then Put Else Get 原子竞争，Else 校验一个结果；Ms 只轮询读取不生成，取消返回命名 err（可能 nil），不可凭预期修正 Go | 3；TestInitClusterID。全局 keypath cache 不照搬；MCS 重试/日志装配归上层主题 8 |
| meta.go Cluster | /pd/C/raft，metapb.Cluster protobuf；Load/SaveMeta 单键，空值为 false | 4；storage_test.go TestBasic |
| meta.go Store | /pd/C/raft/s/%020d，metapb.Store protobuf；删除只移除 Store key，不清权重 | 5；TestBasic |
| meta.go 权重/LoadStores | /pd/C/schedule/store_weight/%020d/{leader,region}，FormatFloat f,-1,64；两次 Save，第一项可能已写而第二项失败。读权重缺失默认 1；Store 以 ID+1 分页 100，end 为 MaxUint64 key（排他）；Offline/Tombstone 兼容映射 Removing/Removed | 6；TestLoadStores、TestStoreWeight |
| meta.go Region 单键 | /pd/C/raft/r/%020d，metapb.Region protobuf；Save 先 EncryptRegion，Load 解码后 DecryptRegion；LoadRegion 坏 protobuf 返回 true+error（不同于 loadProto） | 7 仅非加密；TestBasic；加密引擎归上层，不得把加密 metadata 静默忽略 |
| meta.go LoadRegions/游标 | end=RegionPath(MaxUint64)，初始 limit 10000，范围读错减半至低于 100 才退出。读返回后检查取消，解码/解密后游标设 ID+1，再回调并逐个删除 overlaps；中途失败可能已推进游标/删部分重叠。NextRegionID/ResetRegionID 暴露游标；默认 Flush/Close 为 no-op | 8；TestLoadRegions、TestLoadRegionsToCache、TestLoadRegionsExceedRangeLimit、TestRegionReload；slowLoadRegion 故障点仅静态记录 |
| config.go 普通 / service_middleware.go | /pd/C/config、/pd/C/service_middleware，encoding/json；缺失/空值 false，坏 JSON false+错误；不包含 server/config 默认值和校验 | 9；server/config 消费端测试需在后续任务指定场景复核；完整 Config 嵌套类型准入归服务配置契约计划，禁止透传 JSON 冒充完整配置迁移 |
| config.go Scheduler | /pd/C/scheduler_config/name，原始 bytes/string；LoadAll 只调用一次范围读取上限 100 并去 prefix，不是遍历全部分页 | 10；调度配置消费者；无 endpoint 同目录独立单测 |
| rule.go | /pd/C/rules/id、rule_group/id、region_label/id；写 JSON、删除使用传入 txn；单读原始 string，遍历使用去前缀分页器；同事务由调用者组合，不额外开事务 | 11；pkg/schedule/placement/rule_manager_test.go 等调用方；业务规则算法归调度主题 |
| affinity.go | /pd/C/affinity_groups/id；JSON 写/txn 删除，原始 string 读/分页；复用调用方事务 | 12；pkg/schedule/affinity/manager_test.go；算法及 watcher 生命周期归上层 |
| replication_status.go | /pd/C/replication_mode/mode；JSON，空/缺失 false，坏 JSON false+error | 13；pkg/replication/replication_mode_test.go |
| min_resolved_ts.go / external_timestamp.go | /pd/C/raft/min_resolved_ts、external_timestamp；无前导零小写十六进制 uint64，缺失/空值 0，非法/溢出 ParseUint 错；公开 HTTP 包装 struct 的 JSON 字段不等于持久化格式 | 14；tests/server/api/min_resolved_ts_test.go 等上层 API 消费者；无同目录专项单测 |
| tso.go 读删 | group 0 为 /pd/C/timestamp；其余 /ms/C/tso/%05d/gta/timestamp；8 字节 UnixNano，缺失 ZeroTime，坏长度错误；删除在 caller ctx callback txn 内 | 15；TestSaveLoadTimestamp、TestTimestampTxn |
| tso.go SaveTimestamp | leader value 必须非空；同 callback txn 读 leader value、旧 timestamp，要求 leader 相等且 wall clock 严格增大，然后写；无新 lease。Go 是值保护，不能宣称已经是 term/lease 原子保护 | 16；TestSaveTimestampWithLeaderCheck、TestSaveTimestampWithTimeout。Rust 复用 member Leadership fencing，需显式证明兼容及故障边界 |
| resource_group.go Settings | resource_group/settings/name（N）或 resource_group/keyspace/settings/id/name；gogo protobuf，调用者通常为 rmpb.ResourceGroup；遍历先 legacy 后 keyspace，legacy 传 N，非法路径日志后跳过；单写/删 | 17；TestResourceGroupStorage；坏路径/值场景仍需后续补证据 |
| resource_group.go States/Controller | resource_group/states/name 或 resource_group/keyspace/states/id/name，JSON，遍历返回原始值；resource_group/controller 为 JSON、读返回 raw string；legacy 同上 | 18；TestResourceGroupStorage 仅覆盖本行的 states，不据此声称 controller 字节已验证 |
| resource_group.go ServiceLimit | resource_group/keyspace/service_limits/id，JSON float64；缺失 0；LoadServiceLimit 先读再经 loadJSON 二次读（不是快照）；遍历坏 ID/坏 JSON 日志后跳过 | 19；RM service_limit/metadata_watcher 测试，编码不得用整数十六进制代替 |
| keyspace.go Meta/ID | /pd/C/keyspaces/meta/%08d 为 keyspacepb.KeyspaceMeta protobuf；id/name 为十进制 ID，LoadKeyspaceID 仅接受 uint24。Meta 优先 RevisionTxn.LoadWithRevision，缺失 nil；Remove 在 caller txn 移除元信息和名字映射；不保证两个独立 API 自动共同提交 | 20；TestSaveLoadKeyspace、TestEncodeSpaceID |
| keyspace.go Range | 从 MetaPath(startID) 到 MetaPrefix 的 prefixEnd；limit 0 全量，空结果空 slice，逐项 protobuf | 21；TestLoadRangeKeyspaces |
| tso_keyspace_group.go 基础值 | UserKind Basic=0/Standard=1/Enterprise=2/Count=3，字符串 basic/standard/enterprise；未知输入回 Basic，未知枚举 String 为 unknown UserKind；成员比较地址忽略 scheme。KeyspaceGroup JSON 排除 lookup table，保留 nil/空成员列表差异 | 22；pkg/keyspace/tso_keyspace_group_test.go；真实 JSON 输出仍待取得 |
| tso_keyspace_group.go 拆合并值 | SplitState split-source、MergeState merge-list；nil 安全 IsSplitting/IsMerging；split source=self，merge source=列表含 self，目标相反；SplitSource 无状态返回 0；指针字段 omitempty | 23；同上；算法和选主调度不属值层 |
| tso_keyspace_group.go 存储 | /pd/C/tso/keyspace_groups/membership/%05d，JSON；点读优先 revision 保护，缺失 nil，禁止删默认组；范围 start/limit，空返回空 slice；caller txn 写删 | 24；TestLoadKeyspaceGroupRevisionComparisonKeepsRequestBounded（大值不进入 Value compare） |
| meta_service_group.go | /pd/C/meta_service_groups/id/status；JSON assignment_count/enabled；按 ids 的 key 集合读取，忽略 map value，缺失返回非 nil 零 struct；caller txn 写/删 | 25；TestMetaServiceGroupStorage；删除后重加的消费行为需另证，不把现有缺失断言当删除测试 |
| maintenance.go 启动/读 | /pd/C/maintenance/type JSON；maintenance_lock 值 type/id。RawTxn NotExists(lock) Then 两 Put Else GetRange(prefix,prefix+FF,100)；忽略传入 ctx，不附 lease；Else 排除 lock/坏 JSON，多有效任务错误，至多一个返回 conflict；LoadAll 会包含 lock 原始值 | 26；tests/server/apiv2/handlers/maintenance_test.go 的 TestMaintenanceAPI、TestMaintenanceAPIAtomicOperations、TestMaintenanceAPIMultipleTaskTypes；无 endpoint 同目录专项测试，不能用先读再写实现原子性 |
| maintenance.go 删除 | Value(lock)=type/id Then 两 Delete Else 同范围读；返回同 type 的可解析冲突项，否则 false,nil；忽略 ctx，无锁到期机制 | 27；同一 maintenance_test.go 的错误 ID 冲突、正确 ID 删除及缺失 404 断言；后续 Rust 须实测存储结果，完整维护流程归上层 API |
| gc_states.go 值 | ServiceSafePoint JSON service_id/expired_at/safe_point；keyspace_id 缺失或 null 表示 N，0 是默认空间不可混淆。GCBarrier 转旧 SSP，永不过期 MaxInt64；读 expired_at<=0 或 >=MaxInt64 视为无期限。GlobalGCBarrier 自身 JSON，TimeOptional 以 Unix 秒数编码（nil=0，解码 <=0 或 >=MaxInt64 为 nil）；构造向上取整秒且修改传入时间，IsExpired 严格 now.After，等于边界不算过期 | 28；TestGCBarriersConversions、TestGlobalGCBarriersMarshalling、TestGCBarrierExpiring |
| gc_states.go SafePoint 读 | GCSafePointPath(N)=/pd/C/gc/safe_point，十六进制；其他 /pd/C/keyspaces/gc_safe_point/%08d，JSON keyspace_id/safe_point；TxnSafePoint 是 /tidb/store/gcworker/saved_safe_point 或 /keyspaces/tidb/id/tidb/store/gcworker/saved_safe_point，十进制。缺失空值 0；TiDB minStartTS 扫描十进制取最小，返回去 prefix 标识，同值保留先遇到项 | 29；TestGCSafePoint、TestTxnSafePoint、TestTiDBMinStartTS |
| gc_states.go Barrier 读 | GCBarrier(N) /pd/C/gc/safe_point/service/id；其他 /pd/C/keyspaces/service_safe_point/%08d/id；全局 /pd/C/gc/global_gc_barriers/id；普通解 SSP 转 Barrier，全局直接 JSON；单读缺失 nil，批读不筛 reserved/expired；CompatibleLoadAllServiceGCSafePoints 返回完整 key，空时两个空 slice | 30；TestGCBarrier、TestGlobalGCBarrier、TestGCStateJSONUtil |
| gc_states.go SafePoint 写事务 | GCStateRevisionPath=/pd/C/gc/gc_state_revision，十进制应用 revision（区别于 etcd revision）；先读，再执行 callback 收集 ops，RawTxn NotExists 或 Value(old) 比较，非空写附 revision+1，空写只比较不增号；callback 错误不提交，false 为 ErrEtcdTxnConflict，校验响应项数 | 31；TestGCStateTransactionACID、TestDataPhysicalRepresentation；空字符串的存在键不等于未存在 |
| gc_states.go Barrier 写事务 | GCStateWriteBatch 增删普通/全局 Barrier 与 safe point 混合同一 RawTxn；不自行执行过期清理、不附 lease | 32；同上及各 barrier 测试；GC manager 不变量和调度算法归上层 GC 主题 |
| gc_state_batch.go | 每 scope TxnSafePoint/GCSafePoint 两点读，同一 RawTxn Then；上限 MaxEtcdTxnOps/2=60，空输入无读，重复拒绝；输出输入同序，预取消先拒绝；错误响应全批失败，decode 错归对应结果，Txn decode 失败则不解 GC | 33；TestLoadGCSafePointPairs、BatchBounds、Unsupported、Context、InvalidResponse |
| util.go RunBatchOpInTxn | 每 120 个 callback op 一个 RunInTxn，空 batch 无事务；本批失败阻止后续批，前批不回滚；callback 数不等于实际底层 op 数，不承诺全 batch 原子 | 34；pkg/schedule/affinity/txn_test.go 是间接业务消费者，当前无 endpoint 同目录专项边界测试，需后续跨后端补证据 |
| storage.go 门面/工厂 | Storage 嵌入 Base 与全部业务接口。memory/etcd 工厂传 nil encryption manager；LevelDB 专用工厂接收 manager。NewCoreStorage 仅 Region CRUD/LoadRegions 按 atomic 开关转发，其余继承 default；条件事务转发 default。Retrieve/TrySwitch 非 core 分支分别返回自身/nil；Flush/Close 仅 regionStorage；AreRegionsLoaded 强制 core 断言；IsBootstrapped 读 bootstrap time，读错 false、非空 true | 存储门面计划任务 1–5；TestTryGetLocalRegionStorage、TestTrySwitchRegionStorage、TestLoadRegionsToCache；本子计划不实现工厂、本地引擎/持久化 |

## 现有 Rust、watch 与未就绪依赖

现有 `pkg/storage/lib.rs` 只重导出 canonical `astersql-pd-storage-kv`。
KV 已有 Base/CallbackTxn/RevisionTxn、RawTxn 六比较、Then/Else 有序点/范围响应、
取消分类及唯一 MetaStore 的 read/transact/watch/grant/keep_alive/revoke/ttl/compact。
`kv/lib.rs` 注册独立测试：bounded_range、base_kv、callback_txn、revision_txn、
raw_compare、raw_branches、raw_get、raw_range、txn_cancel、meta_store；
`kv/etcd_store_test.rs` 为真实 etcd target，复用 contract_test 与既有 fixture。
这些测试未覆盖本清单的业务 schema，不能作为 endpoint 字节或行为验收。
keypath 已有 cluster/member_config/rule_resource/keyspace_time/gc/registry_marker 模块
及各独立 `*_test.rs`。`pkg/member/member.rs`、`pkg/id/allocator.rs`、
`pkg/runtime/node.rs` 分别提供 bootstrap/Leadership fencing、ID、宿主装配，
runtime bootstrap_config 已使用 /pd/cluster_id 与 8 字节大端，但候选 ID 使用系统纳秒，
并非 Go 的秒+随机；3 须复用或整理现有路径，不能另建竞争实现。Member 当前身份键为
/pd/C/rust/members/id，现有 bootstrap/fencing 不能等同完整 Go member 迁移；
runtime bootstrap_config 不代表完整 server/config 已迁移。

endpoint 本身没有 watch API。下列生产消费者直接观察相同键空间，不能因只看 endpoint
公开方法漏掉；watch revision/压缩/取消/lease 的底层能力已存在，但业务恢复/缓存同步仍待
上层主题 8（MCS）及主题 5（GC/统计）迁移，不属于本次清单实现。

| 现有消费者文件 | 观察内容与迁移归属 |
| --- | --- |
| `pkg/mcs/scheduling/server/meta/watcher.go`、`pkg/mcs/router/server/meta/watcher.go` | Store protobuf，StorePathPrefix；MCS 业务缓存 |
| `pkg/mcs/scheduling/server/config/watcher.go` | 普通配置、TTL 配置、scheduler；TTL 是上层 configutil/lease 行为，endpoint SaveConfig 不绑定 lease |
| `pkg/mcs/scheduling/server/rule/watcher.go` | rules/rule_group 共用 rule 父前缀、region_label；事务批次/缓存更新 |
| `pkg/mcs/scheduling/server/affinity/watcher.go` | affinity_groups 与 affinity region labels |
| `pkg/mcs/scheduling/server/keyspace_meta/watcher.go` | keyspace protobuf 的初始加载及后续更新 |
| `pkg/keyspace/tso_keyspace_group.go` | membership JSON 与 TSO registry；组值/存储归 22–24，watch/选主装配归上层 MCS |
| `pkg/mcs/resourcemanager/server/metadata_watcher.go` | resource_group/controller、legacy/keyspace settings/states、service_limits；初始加载/put/delete/错误跳过 |
| `pkg/gc/enabled_keyspace_index.go` | keyspace 状态索引的 prefix watch、revision 续接、压缩重载；GC manager |

加密依赖为 `pkg/encryption/region_crypter.go::{EncryptRegion,DecryptRegion}`、
`KeyManager`/`Manager`、encryptionpb metadata 与密钥来源。EncryptRegion 无 manager/无
当前 key 可返回原 region，有 key 克隆后加密边界；DecryptRegion 有 metadata 而无 manager
报错。7/8 只准入非加密路径；加密引擎/KMS/轮换及本地生产 persistence 归上层主题 10，
不宣称通过本子计划替代。`pkg/storage/raft-prototype` 为独立 durability 原型，非生产后端。

完整 Config 的嵌套 schedule/replication/keyspace/resource 等领域类型、默认值、废弃字段、
校验与 PersistOptions 归服务配置契约计划的显式准入门；9 仅持久化适配，18 controller
也不等于完整配置服务。规则/亲和算法、GC manager 不变量、完整 MCS/服务启动均未就绪，
有明确上层责任，不以 raw JSON 或恒成功校验填补。

编码任务 2–34 中凡要求逐字节 oracle 的项目仍须取得**真实 Go 编码输出**；现有 Go
源码/测试字面断言只能作为静态依据。`pkg/proto/wire-fixtures.json` 是既有协议 fixtures，
不涵盖全部 Cluster/Store/Region/Keyspace/resource settings 与各 JSON 值。
没有运行端点编码差分，本任务不宣称这些未来准入证据已经满足；缺失时必须阻塞相应任务，
不得新增 Go 文件。需要 failpoint 的 TestLoadRegionsExceedRangeLimit、TestRegionReload、
TestSaveTimestampWithTimeout 不裸跑，不注入、不改 Go；历史 KV 回归不是它们的新运行证据。

## 来源文件固定与全部公开声明

以下逐个覆盖全部 20 个生产文件（endpoint 19 个及 storage.go）。
每个函数签名包含多行补读；struct/interface 块包含字段、嵌入和方法，常量组整体保留。
私有 helper 的行为已纳入主表；不是将 rg 命中数当行为测试数。

### `pkg/storage/endpoint/affinity.go`

SHA-256：`b2f8c3f6fe1c718fae3d2fdc2d28f6f62e4c82009f11ca3de68abaacb32ec42c`。

keypath 引用：`AffinityGroupPath`、`AffinityGroupsPrefix`。

行 25：

```go
type AffinityStorage interface {
	LoadAffinityGroup(groupID string) (string, error)
	LoadAllAffinityGroups(f func(k, v string)) error
	SaveAffinityGroup(txn kv.Txn, groupID string, group any) error
	DeleteAffinityGroup(txn kv.Txn, groupID string) error
	RunInTxn(ctx context.Context, f func(txn kv.Txn) error) error
}
```

行 43：

```go
func (*StorageEndpoint) SaveAffinityGroup(txn kv.Txn, groupID string, group any) error {
```

行 48：

```go
func (*StorageEndpoint) DeleteAffinityGroup(txn kv.Txn, groupID string) error {
```

行 54：

```go
func (se *StorageEndpoint) LoadAffinityGroup(groupID string) (string, error) {
```

行 60：

```go
func (se *StorageEndpoint) LoadAllAffinityGroups(f func(k, v string)) error {
```

### `pkg/storage/endpoint/cluster_id.go`

SHA-256：`e999033081b896ccdc8db10aed1374fa37a2485527d49b292fed777d5e2ceaf2`。

keypath 引用：`ClusterID`、`ClusterIDPath`、`SetClusterID`。

行 38：

```go
func InitClusterID(c *clientv3.Client) (uint64, error) {
```

行 126：

```go
func InitClusterIDForMs(ctx context.Context, client *clientv3.Client) (err error) {
```

### `pkg/storage/endpoint/config.go`

SHA-256：`80b88b85f84a52a612193c4c1f0b031db28d053f6ef35f8ac69896be4a5ecdec`。

keypath 引用：`Config`、`ConfigPath`、`SchedulerConfigPath`、`SchedulerConfigPathPrefix`。

行 28：

```go
type ConfigStorage interface {
	LoadConfig(cfg any) (bool, error)
	SaveConfig(cfg any) error
	LoadAllSchedulerConfigs() ([]string, []string, error)
	LoadSchedulerConfig(schedulerName string) (string, error)
	SaveSchedulerConfig(schedulerName string, data []byte) error
	RemoveSchedulerConfig(schedulerName string) error
}
```

行 42：

```go
func (se *StorageEndpoint) LoadConfig(cfg any) (bool, error) {
```

行 55：

```go
func (se *StorageEndpoint) SaveConfig(cfg any) error {
```

行 60：

```go
func (se *StorageEndpoint) LoadAllSchedulerConfigs() (keys, values []string, err error) {
```

行 70：

```go
func (se *StorageEndpoint) LoadSchedulerConfig(schedulerName string) (string, error) {
```

行 75：

```go
func (se *StorageEndpoint) SaveSchedulerConfig(schedulerName string, data []byte) error {
```

行 80：

```go
func (se *StorageEndpoint) RemoveSchedulerConfig(schedulerName string) error {
```

### `pkg/storage/endpoint/endpoint.go`

SHA-256：`a80a322e2edd66d55d3407eafac44e78ac02f989c8709f408ca8d44d6f61b74a`。

keypath 引用：无业务 key 构造。

行 33：

```go
type StorageEndpoint struct {
	kv.Base
	encryptionKeyManager *encryption.Manager
	nextRegionID uint64
}
```

行 42：

```go
func NewStorageEndpoint(
	kvBase kv.Base,
	encryptionKeyManager *encryption.Manager,
) *StorageEndpoint {
```

行 72：

```go
func (se *StorageEndpoint) RunInTxnWithConditions(
	ctx context.Context,
	conditions []clientv3.Cmp,
	f func(txn kv.Txn) error,
) error {
```

行 130：

```go
func (se *StorageEndpoint) NextRegionID() uint64 {
```

行 135：

```go
func (se *StorageEndpoint) ResetRegionID() {
```

### `pkg/storage/endpoint/external_timestamp.go`

SHA-256：`af197109719a3fac3d6d11670c60a86dab655605694c256db9a3e028b25ef391`。

keypath 引用：`ExternalTimestampPath`。

行 26：

```go
type ExternalTimestamp struct {
	ExternalTimestamp uint64 `json:"external_timestamp"`
}
```

行 31：

```go
type ExternalTSStorage interface {
	LoadExternalTS() (uint64, error)
	SaveExternalTS(timestamp uint64) error
}
```

行 39：

```go
func (se *StorageEndpoint) LoadExternalTS() (uint64, error) {
```

行 52：

```go
func (se *StorageEndpoint) SaveExternalTS(timestamp uint64) error {
```

### `pkg/storage/endpoint/gc_state_batch.go`

SHA-256：`ed68de6055e24b936fadcbd9e9db2f1cc049a41ac27443924b15a4fc6374974f`。

keypath 引用：`GCSafePointPath`、`TxnSafePointPath`。

行 29：

```go
const MaxGCSafePointBatchSize = etcdutil.MaxEtcdTxnOps / 2
```

行 33：

```go
type GCSafePointReadResult struct {
	KeyspaceID   uint32
	TxnSafePoint uint64
	GCSafePoint  uint64
	Err          error
}
```

行 45：

```go
func (p GCStateProvider) LoadGCSafePointPairs(ctx context.Context, keyspaceIDs []uint32) ([]GCSafePointReadResult, error) {
```

### `pkg/storage/endpoint/gc_states.go`

SHA-256：`5bab038897a0c9145817f396dfcbc525881aa4692522575657e2d48adc82ebca`。

keypath 引用：`CompatibleTiDBMinStartTSPrefix`、`GCBarrierPath`、`GCBarrierPrefix`、`GCSafePointPath`、`GCStateRevisionPath`、`GlobalGCBarrierPath`、`GlobalGCBarrierPrefix`、`TxnSafePointPath`。

行 51：

```go
type ServiceSafePoint struct {
	ServiceID string
	ExpiredAt int64
	SafePoint uint64
	KeyspaceID uint32
}
```

行 72：

```go
func (s *ServiceSafePoint) MarshalJSON() ([]byte, error) {
```

行 98：

```go
func (s *ServiceSafePoint) UnmarshalJSON(data []byte) error {
```

行 128：

```go
type GlobalGCBarrier struct {
	BarrierID      string                `json:"barrier_id"`
	BarrierTS      uint64                `json:"barrier_ts"`
	ExpirationTime typeutil.TimeOptional `json:"expiration_time"`
}
```

行 137：

```go
func NewGlobalGCBarrier(barrierID string, barrierTS uint64, expirationTime *time.Time) *GlobalGCBarrier {
```

行 151：

```go
func (b *GlobalGCBarrier) IsExpired(now time.Time) bool {
```

行 156：

```go
func (b *GlobalGCBarrier) String() string {
```

行 168：

```go
type GCBarrier struct {
	BarrierID string
	BarrierTS uint64
	ExpirationTime *time.Time
}
```

行 178：

```go
func NewGCBarrier(barrierID string, barrierTS uint64, expirationTime *time.Time) *GCBarrier {
```

行 214：

```go
func (b *GCBarrier) ToServiceSafePoint(keyspaceID uint32) *ServiceSafePoint {
```

行 228：

```go
func (b *GCBarrier) IsExpired(now time.Time) bool {
```

行 233：

```go
func (b *GCBarrier) String() string {
```

行 245：

```go
type GCStateStorage interface {
	GetGCStateProvider() GCStateProvider
}
```

行 253：

```go
func (se *StorageEndpoint) GetGCStateProvider() GCStateProvider {
```

行 261：

```go
type GCStateProvider struct {
	storage *StorageEndpoint
}
```

行 272：

```go
type GCStateWriteBatch struct {
	ops []kv.RawTxnOp
}
```

行 277：

```go
func (p GCStateProvider) LoadGCSafePoint(keyspaceID uint32) (uint64, error) {
```

行 337：

```go
func (p GCStateProvider) LoadTxnSafePoint(keyspaceID uint32) (uint64, error) {
```

行 358：

```go
func (p GCStateProvider) LoadGCBarrier(keyspaceID uint32, barrierID string) (*GCBarrier, error) {
```

行 370：

```go
func (p GCStateProvider) LoadAllGCBarriers(keyspaceID uint32) ([]*GCBarrier, error) {
```

行 388：

```go
func (p GCStateProvider) LoadGlobalGCBarrier(barrierID string) (*GlobalGCBarrier, error) {
```

行 398：

```go
func (p GCStateProvider) LoadAllGlobalGCBarriers() ([]*GlobalGCBarrier, error) {
```

行 413：

```go
func (p GCStateProvider) CompatibleLoadTiDBMinStartTS(keyspaceID uint32) (string, uint64, error) {
```

行 455：

```go
func (p GCStateProvider) RunInGCStateTransaction(f func(wb *GCStateWriteBatch) error) error {
```

行 514：

```go
func (p GCStateProvider) CompatibleLoadAllServiceGCSafePoints(keyspaceID uint32) ([]string, []*ServiceSafePoint, error) {
```

行 543：

```go
func (wb *GCStateWriteBatch) SetGCSafePoint(keyspaceID uint32, gcSafePoint uint64) error {
```

行 571：

```go
func (wb *GCStateWriteBatch) SetTxnSafePoint(keyspaceID uint32, txnSafePoint uint64) error {
```

行 583：

```go
func (wb *GCStateWriteBatch) SetGlobalGCBarrier(barrier *GlobalGCBarrier) error {
```

行 589：

```go
func (wb *GCStateWriteBatch) DeleteGlobalGCBarrier(barrierID string) error {
```

行 599：

```go
func (wb *GCStateWriteBatch) SetGCBarrier(keyspaceID uint32, newGCBarrier *GCBarrier) error {
```

行 605：

```go
func (wb *GCStateWriteBatch) DeleteGCBarrier(keyspaceID uint32, barrierID string) error {
```

### `pkg/storage/endpoint/keyspace.go`

SHA-256：`1fc82600b7ca2ff87b5e6b9efd280e82634556a76993716286ef88a5c25e1dec`。

keypath 引用：`KeyspaceIDPath`、`KeyspaceMetaPath`、`KeyspaceMetaPrefix`。

行 31：

```go
const (
	SpaceIDBase = 10
	spaceIDBitSizeMax = 24
)
```

行 42：

```go
type KeyspaceStorage interface {
	SaveKeyspaceMeta(txn kv.Txn, meta *keyspacepb.KeyspaceMeta) error
	LoadKeyspaceMeta(txn kv.Txn, id uint32) (*keyspacepb.KeyspaceMeta, error)
	SaveKeyspaceID(txn kv.Txn, id uint32, name string) error
	LoadKeyspaceID(txn kv.Txn, name string) (bool, uint32, error)
	RemoveKeyspace(txn kv.Txn, id uint32, name string) error
	LoadRangeKeyspace(txn kv.Txn, startID uint32, limit int) ([]*keyspacepb.KeyspaceMeta, error)
	RunInTxn(ctx context.Context, f func(txn kv.Txn) error) error
}
```

行 56：

```go
func (*StorageEndpoint) SaveKeyspaceMeta(txn kv.Txn, meta *keyspacepb.KeyspaceMeta) error {
```

行 67：

```go
func (*StorageEndpoint) LoadKeyspaceMeta(txn kv.Txn, id uint32) (*keyspacepb.KeyspaceMeta, error) {
```

行 90：

```go
func (*StorageEndpoint) SaveKeyspaceID(txn kv.Txn, id uint32, name string) error {
```

行 97：

```go
func (*StorageEndpoint) RemoveKeyspace(txn kv.Txn, id uint32, name string) error {
```

行 110：

```go
func (*StorageEndpoint) LoadKeyspaceID(txn kv.Txn, name string) (bool, uint32, error) {
```

行 126：

```go
func (*StorageEndpoint) LoadRangeKeyspace(txn kv.Txn, startID uint32, limit int) ([]*keyspacepb.KeyspaceMeta, error) {
```

### `pkg/storage/endpoint/maintenance.go`

SHA-256：`0ca713699c1acda65d95f6f1b05d8eb1524a359a79c9daea21705b05048622d2`。

keypath 引用：`MaintenanceTaskPath`、`MaintenanceTaskPathPrefix`。

行 39：

```go
const MaintenanceLockName = "maintenance_lock"
```

行 43：

```go
type MaintenanceTask struct {
	Type           string `json:"type"`            // The type of maintenance task
	ID             string `json:"id"`              // Unique identifier for the task
	StartTimestamp int64  `json:"start_timestamp"` // Unix timestamp when the task started
	Description    string `json:"description"`     // Optional description of the task
}
```

行 51：

```go
type MaintenanceStorage interface {
	LoadMaintenanceTask(taskType string) (*MaintenanceTask, error)
	LoadAllMaintenanceTasks(f func(k, v string)) error
	TryStartMaintenanceTaskAtomic(ctx context.Context, task *MaintenanceTask) (bool, *MaintenanceTask, error)
	TryDeleteMaintenanceTaskAtomic(ctx context.Context, taskType, taskID string) (bool, *MaintenanceTask, error)
}
```

行 65：

```go
func (se *StorageEndpoint) LoadMaintenanceTask(taskType string) (*MaintenanceTask, error) {
```

行 82：

```go
func (se *StorageEndpoint) LoadAllMaintenanceTasks(f func(k, v string)) error {
```

行 88：

```go
func (se *StorageEndpoint) TryStartMaintenanceTaskAtomic(_ context.Context, task *MaintenanceTask) (bool, *MaintenanceTask, error) {
```

行 190：

```go
func (se *StorageEndpoint) TryDeleteMaintenanceTaskAtomic(_ context.Context, taskType, taskID string) (bool, *MaintenanceTask, error) {
```

### `pkg/storage/endpoint/meta.go`

SHA-256：`408e661468c0db03676d895e888abb66519caaf65dd03a3e4adb46c1fef02f86`。

keypath 引用：`ClusterPath`、`RegionPath`、`StoreLeaderWeightPath`、`StorePath`、`StoreRegionWeightPath`。

行 35：

```go
type MetaStorage interface {
	LoadMeta(meta *metapb.Cluster) (bool, error)
	SaveMeta(meta *metapb.Cluster) error
	LoadStoreMeta(storeID uint64, store *metapb.Store) (bool, error)
	SaveStoreMeta(store *metapb.Store) error
	SaveStoreWeight(storeID uint64, leader, region float64) error
	LoadStores(f func(store *core.StoreInfo, opts ...core.StoreCreateOption)) error
	DeleteStoreMeta(store *metapb.Store) error
	RegionStorage
}
```

行 47：

```go
type RegionStorage interface {
	LoadRegion(regionID uint64, region *metapb.Region) (ok bool, err error)
	LoadRegions(ctx context.Context, f func(region *core.RegionInfo) []*core.RegionInfo) error
	SaveRegion(region *metapb.Region) error
	DeleteRegion(region *metapb.Region) error
	Flush() error
	Close() error
}
```

行 58：

```go
const (
	MaxKVRangeLimit = 10000
	MinKVRangeLimit = 100
)
```

行 67：

```go
func (se *StorageEndpoint) LoadMeta(meta *metapb.Cluster) (bool, error) {
```

行 73：

```go
func (se *StorageEndpoint) SaveMeta(meta *metapb.Cluster) error {
```

行 78：

```go
func (se *StorageEndpoint) LoadStoreMeta(storeID uint64, store *metapb.Store) (bool, error) {
```

行 83：

```go
func (se *StorageEndpoint) SaveStoreMeta(store *metapb.Store) error {
```

行 88：

```go
func (se *StorageEndpoint) SaveStoreWeight(storeID uint64, leader, region float64) error {
```

行 98：

```go
func (se *StorageEndpoint) LoadStores(putStore func(store *core.StoreInfo, opts ...core.StoreCreateOption)) error {
```

行 155：

```go
func (se *StorageEndpoint) DeleteStoreMeta(store *metapb.Store) error {
```

行 160：

```go
func (se *StorageEndpoint) LoadRegion(regionID uint64, region *metapb.Region) (ok bool, err error) {
```

行 174：

```go
func (se *StorageEndpoint) LoadRegions(ctx context.Context, f func(region *core.RegionInfo) []*core.RegionInfo) error {
```

行 225：

```go
func (se *StorageEndpoint) SaveRegion(region *metapb.Region) error {
```

行 238：

```go
func (se *StorageEndpoint) DeleteRegion(region *metapb.Region) error {
```

行 243：

```go
func (*StorageEndpoint) Flush() error
```

行 246：

```go
func (*StorageEndpoint) Close() error
```

### `pkg/storage/endpoint/meta_service_group.go`

SHA-256：`be3ab6af2aefb6696a2fd18f788616f3b9b96bc447bc3503ed075dba6397503f`。

keypath 引用：`MetaServiceGroupStatusPath`。

行 26：

```go
type MetaServiceGroupStorage interface {
	SaveMetaServiceGroupStatus(txn kv.Txn, id string, status *MetaServiceGroupStatus) error
	LoadMetaServiceGroupStatus(txn kv.Txn, ids map[string]string) (map[string]*MetaServiceGroupStatus, error)
	RemoveMetaServiceGroupStatus(txn kv.Txn, id string) error
	RunInTxn(ctx context.Context, f func(txn kv.Txn) error) error
}
```

行 35：

```go
type MetaServiceGroupStatus struct {
	AssignmentCount int  `json:"assignment_count"`
	Enabled         bool `json:"enabled"`
}
```

行 41：

```go
func (*StorageEndpoint) SaveMetaServiceGroupStatus(txn kv.Txn, id string, status *MetaServiceGroupStatus) error {
```

行 51：

```go
func (*StorageEndpoint) LoadMetaServiceGroupStatus(txn kv.Txn, ids map[string]string) (map[string]*MetaServiceGroupStatus, error) {
```

行 66：

```go
func (*StorageEndpoint) RemoveMetaServiceGroupStatus(txn kv.Txn, id string) error {
```

### `pkg/storage/endpoint/min_resolved_ts.go`

SHA-256：`54bcf77a022955bc7d50e729f436b3a898a1a79989634a1ca83db94a18680a35`。

keypath 引用：`MinResolvedTSPath`。

行 26：

```go
type MinResolvedTSPoint struct {
	MinResolvedTS uint64 `json:"min_resolved_ts"`
}
```

行 31：

```go
type MinResolvedTSStorage interface {
	LoadMinResolvedTS() (uint64, error)
	SaveMinResolvedTS(minResolvedTS uint64) error
}
```

行 39：

```go
func (se *StorageEndpoint) LoadMinResolvedTS() (uint64, error) {
```

行 52：

```go
func (se *StorageEndpoint) SaveMinResolvedTS(minResolvedTS uint64) error {
```

### `pkg/storage/endpoint/replication_status.go`

SHA-256：`fd0d7dbb0bf07fab2eff6bb52f6e2b8dd76e3edcfe0b6b85fa8656c44cc04a08`。

keypath 引用：`ReplicationModePath`。

行 25：

```go
type ReplicationStatusStorage interface {
	LoadReplicationStatus(mode string, status any) (bool, error)
	SaveReplicationStatus(mode string, status any) error
}
```

行 33：

```go
func (se *StorageEndpoint) LoadReplicationStatus(mode string, status any) (bool, error) {
```

行 46：

```go
func (se *StorageEndpoint) SaveReplicationStatus(mode string, status any) error {
```

### `pkg/storage/endpoint/resource_group.go`

SHA-256：`04f7141302601a2cb056df4260abe78fcaf125360b976d40a35ecf42a9edf1e1`。

keypath 引用：`ControllerConfigPath`、`KeyspaceResourceGroupSettingPath`、`KeyspaceResourceGroupSettingPrefix`、`KeyspaceResourceGroupStatePath`、`KeyspaceResourceGroupStatePrefix`、`KeyspaceServiceLimitPath`、`KeyspaceServiceLimitPrefix`、`ParseKeyspaceResourceGroupPath`、`ResourceGroupSettingPrefix`、`ResourceGroupStatePrefix`。

行 31：

```go
type ResourceGroupStorage interface {
	LoadResourceGroupSettings(f func(keyspaceID uint32, name, rawValue string)) error
	SaveResourceGroupSetting(keyspaceID uint32, name string, msg proto.Message) error
	DeleteResourceGroupSetting(keyspaceID uint32, name string) error
	LoadResourceGroupStates(f func(keyspaceID uint32, name, rawValue string)) error
	SaveResourceGroupStates(keyspaceID uint32, name string, obj any) error
	DeleteResourceGroupStates(keyspaceID uint32, name string) error
	SaveControllerConfig(config any) error
	LoadControllerConfig() (string, error)
	LoadServiceLimit(keyspaceID uint32) (float64, error)
	SaveServiceLimit(keyspaceID uint32, serviceLimit float64) error
	LoadServiceLimits(f func(keyspaceID uint32, serviceLimit float64)) error
}
```

行 48：

```go
func (se *StorageEndpoint) SaveResourceGroupSetting(keyspaceID uint32, name string, msg proto.Message) error {
```

行 53：

```go
func (se *StorageEndpoint) DeleteResourceGroupSetting(keyspaceID uint32, name string) error {
```

行 58：

```go
func (se *StorageEndpoint) LoadResourceGroupSettings(f func(keyspaceID uint32, name string, rawValue string)) error {
```

行 77：

```go
func (se *StorageEndpoint) SaveResourceGroupStates(keyspaceID uint32, name string, obj any) error {
```

行 82：

```go
func (se *StorageEndpoint) DeleteResourceGroupStates(keyspaceID uint32, name string) error {
```

行 87：

```go
func (se *StorageEndpoint) LoadResourceGroupStates(f func(keyspaceID uint32, name string, rawValue string)) error {
```

行 106：

```go
func (se *StorageEndpoint) SaveControllerConfig(config any) error {
```

行 111：

```go
func (se *StorageEndpoint) LoadControllerConfig() (string, error) {
```

行 116：

```go
func (se *StorageEndpoint) LoadServiceLimit(keyspaceID uint32) (float64, error) {
```

行 125：

```go
func (se *StorageEndpoint) SaveServiceLimit(keyspaceID uint32, serviceLimit float64) error {
```

行 130：

```go
func (se *StorageEndpoint) LoadServiceLimits(f func(keyspaceID uint32, serviceLimit float64)) error {
```

### `pkg/storage/endpoint/rule.go`

SHA-256：`e944b020f8ab27e9d578aa751ad4118de2c96ce523a6006f33c78e7295d4ec13`。

keypath 引用：`RegionLabelKeyPath`、`RegionLabelPathPrefix`、`RuleGroupIDPath`、`RuleGroupPathPrefix`、`RuleKeyPath`、`RulesPathPrefix`。

行 25：

```go
type RuleStorage interface {
	LoadRule(ruleKey string) (string, error)
	LoadRules(f func(k, v string)) error
	LoadRuleGroups(f func(k, v string)) error
	LoadRegionRules(f func(k, v string)) error
	SaveRule(txn kv.Txn, ruleKey string, rule any) error
	DeleteRule(txn kv.Txn, ruleKey string) error
	SaveRuleGroup(txn kv.Txn, groupID string, group any) error
	DeleteRuleGroup(txn kv.Txn, groupID string) error
	SaveRegionRule(txn kv.Txn, ruleKey string, rule any) error
	DeleteRegionRule(txn kv.Txn, ruleKey string) error
	RunInTxn(ctx context.Context, f func(txn kv.Txn) error) error
}
```

行 48：

```go
func (*StorageEndpoint) SaveRule(txn kv.Txn, ruleKey string, rule any) error {
```

行 53：

```go
func (*StorageEndpoint) DeleteRule(txn kv.Txn, ruleKey string) error {
```

行 58：

```go
func (se *StorageEndpoint) LoadRuleGroups(f func(k, v string)) error {
```

行 63：

```go
func (*StorageEndpoint) SaveRuleGroup(txn kv.Txn, groupID string, group any) error {
```

行 68：

```go
func (*StorageEndpoint) DeleteRuleGroup(txn kv.Txn, groupID string) error {
```

行 73：

```go
func (se *StorageEndpoint) LoadRegionRules(f func(k, v string)) error {
```

行 78：

```go
func (*StorageEndpoint) SaveRegionRule(txn kv.Txn, ruleKey string, rule any) error {
```

行 83：

```go
func (*StorageEndpoint) DeleteRegionRule(txn kv.Txn, ruleKey string) error {
```

行 88：

```go
func (se *StorageEndpoint) LoadRule(ruleKey string) (string, error) {
```

行 93：

```go
func (se *StorageEndpoint) LoadRules(f func(k, v string)) error {
```

### `pkg/storage/endpoint/service_middleware.go`

SHA-256：`95d01d5e28e27210574ef90ebba7b1a4b5200647bda5f11d759337f56003a549`。

keypath 引用：`ServiceMiddlewarePath`。

行 25：

```go
type ServiceMiddlewareStorage interface {
	LoadServiceMiddlewareConfig(cfg any) (bool, error)
	SaveServiceMiddlewareConfig(cfg any) error
}
```

行 33：

```go
func (se *StorageEndpoint) LoadServiceMiddlewareConfig(cfg any) (bool, error) {
```

行 46：

```go
func (se *StorageEndpoint) SaveServiceMiddlewareConfig(cfg any) error {
```

### `pkg/storage/endpoint/tso.go`

SHA-256：`3301344fb56e0eaa7612db39123c739b69a52ffd1a2a08cdbea16c2c6fdf237a`。

keypath 引用：`TimestampPath`。

行 34：

```go
type TSOStorage interface {
	LoadTimestamp(groupID uint32) (time.Time, error)
	SaveTimestamp(ctx context.Context, groupID uint32, ts time.Time, leadership *election.Leadership) error
	DeleteTimestamp(ctx context.Context, groupID uint32) error
}
```

行 47：

```go
func (se *StorageEndpoint) LoadTimestamp(groupID uint32) (time.Time, error) {
```

行 71：

```go
func (se *StorageEndpoint) SaveTimestamp(ctx context.Context, groupID uint32, ts time.Time, leadership *election.Leadership) error {
```

行 117：

```go
func (se *StorageEndpoint) DeleteTimestamp(ctx context.Context, groupID uint32) error {
```

### `pkg/storage/endpoint/tso_keyspace_group.go`

SHA-256：`d628861f2265cb37d9f358f26a5b9bde2f54ba48e66f8f53087659f6f75a22d1`。

keypath 引用：`KeyspaceGroupIDPath`、`KeyspaceGroupIDPrefix`。

行 32：

```go
type UserKind int
```

行 35：

```go
const (
	Basic UserKind = iota
	Standard
	Enterprise
	UserKindCount
)
```

行 44：

```go
func StringUserKind(input string) UserKind {
```

行 57：

```go
func (k UserKind) String() string {
```

行 70：

```go
func IsUserKindValid(kind string) bool {
```

行 83：

```go
type KeyspaceGroupMember struct {
	Address  string `json:"address"`
	Priority int    `json:"priority"`
}
```

行 92：

```go
func (m *KeyspaceGroupMember) IsAddressEquivalent(addr string) bool {
```

行 97：

```go
type SplitState struct {
	SplitSource uint32 `json:"split-source"`
}
```

行 105：

```go
type MergeState struct {
	MergeList []uint32 `json:"merge-list"`
}
```

行 111：

```go
type KeyspaceGroup struct {
	ID       uint32 `json:"id"`
	UserKind string `json:"user-kind"`
	SplitState *SplitState `json:"split-state,omitempty"`
	MergeState *MergeState `json:"merge-state,omitempty"`
	Members []KeyspaceGroupMember `json:"members"`
	Keyspaces []uint32 `json:"keyspaces"`
	KeyspaceLookupTable map[uint32]struct{} `json:"-"`
}
```

行 128：

```go
func (kg *KeyspaceGroup) IsSplitting() bool {
```

行 133：

```go
func (kg *KeyspaceGroup) IsSplitTarget() bool {
```

行 138：

```go
func (kg *KeyspaceGroup) IsSplitSource() bool {
```

行 144：

```go
func (kg *KeyspaceGroup) SplitSource() uint32 {
```

行 152：

```go
func (kg *KeyspaceGroup) IsMerging() bool {
```

行 157：

```go
func (kg *KeyspaceGroup) IsMergeTarget() bool {
```

行 162：

```go
func (kg *KeyspaceGroup) IsMergeSource() bool {
```

行 167：

```go
type KeyspaceGroupStorage interface {
	LoadKeyspaceGroups(startID uint32, limit int) ([]*KeyspaceGroup, error)
	LoadKeyspaceGroup(txn kv.Txn, id uint32) (*KeyspaceGroup, error)
	SaveKeyspaceGroup(txn kv.Txn, kg *KeyspaceGroup) error
	DeleteKeyspaceGroup(txn kv.Txn, id uint32) error
	RunInTxn(ctx context.Context, f func(txn kv.Txn) error) error
}
```

行 179：

```go
func (*StorageEndpoint) LoadKeyspaceGroup(txn kv.Txn, id uint32) (*KeyspaceGroup, error) {
```

行 201：

```go
func (*StorageEndpoint) SaveKeyspaceGroup(txn kv.Txn, kg *KeyspaceGroup) error {
```

行 206：

```go
func (*StorageEndpoint) DeleteKeyspaceGroup(txn kv.Txn, id uint32) error {
```

行 215：

```go
func (se *StorageEndpoint) LoadKeyspaceGroups(startID uint32, limit int) ([]*KeyspaceGroup, error) {
```

### `pkg/storage/endpoint/util.go`

SHA-256：`fd42c0c95d8b826eba0df43446f564d369eb0e55209445fe54ea78f8f66b9eea`。

keypath 引用：无业务 key 构造。

行 82：

```go
type TxnStorage interface {
	RunInTxn(ctx context.Context, f func(txn kv.Txn) error) error
}
```

行 88：

```go
func RunBatchOpInTxn(ctx context.Context, storage TxnStorage, batch []func(kv.Txn) error) error {
```

### `pkg/storage/storage.go`

SHA-256：`35f90a410efe7db0b6f31c5a8cac78d7e3f7fd58d1187b9d7472b3acd00b3530`。

keypath 引用：`ClusterBootstrapTimePath`。

行 35：

```go
type Storage interface {
	kv.Base
	endpoint.ServiceMiddlewareStorage
	endpoint.ConfigStorage
	endpoint.MetaStorage
	endpoint.RuleStorage
	endpoint.ReplicationStatusStorage
	endpoint.GCStateStorage
	endpoint.MinResolvedTSStorage
	endpoint.ExternalTSStorage
	endpoint.KeyspaceStorage
	endpoint.ResourceGroupStorage
	endpoint.TSOStorage
	endpoint.KeyspaceGroupStorage
	endpoint.MetaServiceGroupStorage
	endpoint.MaintenanceStorage
	endpoint.AffinityStorage
}
```

行 57：

```go
func NewStorageWithMemoryBackend() Storage {
```

行 62：

```go
func NewStorageWithEtcdBackend(client *clientv3.Client) Storage {
```

行 68：

```go
func NewRegionStorageWithLevelDBBackend(
	ctx context.Context,
	filePath string,
	ekm *encryption.Manager,
) (*RegionStorage, error) {
```

行 82：

```go
const (
	unloaded regionSource = iota
	fromEtcd
	fromLeveldb
)
```

行 101：

```go
func NewCoreStorage(defaultStorage Storage, regionStorage endpoint.RegionStorage) Storage {
```

行 110：

```go
func (ps *coreStorage) RunInTxnWithConditions(
	ctx context.Context,
	conditions []clientv3.Cmp,
	f func(txn kv.Txn) error,
) error {
```

行 127：

```go
func RetrieveRegionStorage(s Storage) endpoint.RegionStorage {
```

行 139：

```go
func TrySwitchRegionStorage(s Storage, useLocalRegionStorage bool) endpoint.RegionStorage {
```

行 159：

```go
func TryLoadRegionsOnce(ctx context.Context, s Storage, f func(region *core.RegionInfo) []*core.RegionInfo) error {
```

行 186：

```go
func (ps *coreStorage) LoadRegion(regionID uint64, region *metapb.Region) (ok bool, err error) {
```

行 194：

```go
func (ps *coreStorage) LoadRegions(ctx context.Context, f func(region *core.RegionInfo) []*core.RegionInfo) error {
```

行 202：

```go
func (ps *coreStorage) SaveRegion(region *metapb.Region) error {
```

行 210：

```go
func (ps *coreStorage) DeleteRegion(region *metapb.Region) error {
```

行 219：

```go
func (ps *coreStorage) Flush() error {
```

行 228：

```go
func (ps *coreStorage) Close() error {
```

行 236：

```go
func AreRegionsLoaded(s Storage) bool {
```

行 247：

```go
func IsBootstrapped(s Storage) bool {
```

## 直接反向依赖（精确 import）

下表由全部受跟踪 Go 文件精确 import 提取，并列实际包限定符；测试行仅作静态消费依据。
接口嵌入后的间接调用由 storage.go 门面及主表归属传递；不是全部生产调用图。
Go 文件只读，未运行 go list 或构建脚本。

| 文件 | 直接 endpoint 符号 |
| --- | --- |
| `pkg/gc/gc_state_loader.go` | `GCSafePointReadResult`, `MaxGCSafePointBatchSize` |
| `pkg/gc/gc_state_loader_test.go` | `GCSafePointReadResult`, `GCStateWriteBatch` |
| `pkg/gc/gc_state_manager.go` | `GCBarrier`, `GCStateProvider`, `GCStateWriteBatch`, `GlobalGCBarrier`, `NewGCBarrier`, `NewGlobalGCBarrier`, `ServiceSafePoint` |
| `pkg/gc/gc_state_manager_test.go` | `GCBarrier`, `GCStateProvider`, `GCStateWriteBatch`, `GlobalGCBarrier`, `NewGCBarrier`, `NewGlobalGCBarrier`, `NewStorageEndpoint`, `ServiceSafePoint`, `StorageEndpoint` |
| `pkg/gc/gc_state_warmup.go` | `MaxGCSafePointBatchSize` |
| `pkg/gc/gc_state_warmup_scheduler_test.go` | `GCStateProvider`, `MaxGCSafePointBatchSize` |
| `pkg/gc/gc_state_warmup_test.go` | `GCStateWriteBatch` |
| `pkg/gc/metrics.go` | `GCBarrier`, `GlobalGCBarrier` |
| `pkg/gc/metrics_test.go` | `GCBarrier`, `GCStateProvider`, `GCStateWriteBatch`, `GlobalGCBarrier`, `KeyspaceGroup`, `NewGCBarrier`, `NewGlobalGCBarrier`, `NewStorageEndpoint`, `Standard` |
| `pkg/keyspace/keyspace.go` | `Basic`, `KeyspaceGroup`, `KeyspaceStorage`, `StringUserKind` |
| `pkg/keyspace/keyspace_test.go` | `KeyspaceGroup`, `MetaServiceGroupStorage`, `NewStorageEndpoint`, `Standard`, `StorageEndpoint` |
| `pkg/keyspace/meta_service_group.go` | `MetaServiceGroupStatus`, `MetaServiceGroupStorage` |
| `pkg/keyspace/meta_service_group_test.go` | `MetaServiceGroupStatus`, `NewStorageEndpoint` |
| `pkg/keyspace/tso_keyspace_group.go` | `Basic`, `KeyspaceGroup`, `KeyspaceGroupMember`, `KeyspaceGroupStorage`, `MergeState`, `SplitState`, `StringUserKind`, `UserKind`, `UserKindCount` |
| `pkg/keyspace/tso_keyspace_group_test.go` | `Basic`, `Enterprise`, `KeyspaceGroup`, `KeyspaceGroupMember`, `MergeState`, `NewStorageEndpoint`, `SplitState`, `Standard`, `StorageEndpoint` |
| `pkg/keyspace/util.go` | `KeyspaceGroup`, `SpaceIDBase` |
| `pkg/keyspace/util_test.go` | `NewStorageEndpoint` |
| `pkg/mcs/resourcemanager/server/keyspace_manager.go` | `ResourceGroupStorage` |
| `pkg/mcs/resourcemanager/server/manager.go` | `KeyspaceStorage`, `NewStorageEndpoint`, `ResourceGroupStorage` |
| `pkg/mcs/resourcemanager/server/resource_group.go` | `ResourceGroupStorage` |
| `pkg/mcs/resourcemanager/server/service_limit.go` | `ResourceGroupStorage` |
| `pkg/mcs/scheduling/server/affinity/watcher_test.go` | `NewStorageEndpoint` |
| `pkg/mcs/scheduling/server/config/config.go` | `ConfigStorage` |
| `pkg/mcs/scheduling/server/grpc_service_test.go` | `NewStorageEndpoint` |
| `pkg/mcs/scheduling/server/rule/watcher.go` | `RuleStorage` |
| `pkg/mcs/scheduling/server/rule/watcher_test.go` | `NewStorageEndpoint`, `StorageEndpoint` |
| `pkg/mcs/scheduling/server/server.go` | `NewStorageEndpoint`, `StorageEndpoint` |
| `pkg/mcs/scheduling/server/server_test.go` | `NewStorageEndpoint` |
| `pkg/mcs/tso/server/apis/v1/api.go` | `KeyspaceGroup` |
| `pkg/mcs/utils/util.go` | `InitClusterIDForMs` |
| `pkg/replication/replication_mode.go` | `ReplicationStatusStorage` |
| `pkg/schedule/affinity/manager.go` | `AffinityStorage` |
| `pkg/schedule/affinity/txn.go` | `RunBatchOpInTxn` |
| `pkg/schedule/affinity/txn_test.go` | `RunBatchOpInTxn` |
| `pkg/schedule/config/config_provider.go` | `ConfigStorage` |
| `pkg/schedule/labeler/keyspace_index_test.go` | `NewStorageEndpoint` |
| `pkg/schedule/labeler/labeler.go` | `RuleStorage`, `RunBatchOpInTxn` |
| `pkg/schedule/labeler/labeler_test.go` | `NewStorageEndpoint` |
| `pkg/schedule/labeler/plan_test.go` | `NewStorageEndpoint` |
| `pkg/schedule/placement/rule_manager.go` | `RuleStorage`, `RunBatchOpInTxn` |
| `pkg/schedule/placement/rule_manager_test.go` | `NewStorageEndpoint`, `RuleStorage` |
| `pkg/schedule/schedulers/config.go` | `ConfigStorage` |
| `pkg/schedule/schedulers/hot_region_test.go` | `ConfigStorage` |
| `pkg/schedule/schedulers/init.go` | `ConfigStorage` |
| `pkg/schedule/schedulers/scheduler.go` | `ConfigStorage` |
| `pkg/schedule/schedulers/scheduler_controller.go` | `ConfigStorage` |
| `pkg/storage/etcd_backend.go` | `NewStorageEndpoint`, `StorageEndpoint` |
| `pkg/storage/leveldb_backend.go` | `NewStorageEndpoint`, `StorageEndpoint` |
| `pkg/storage/memory_backend.go` | `NewStorageEndpoint`, `StorageEndpoint` |
| `pkg/storage/meta_service_group_test.go` | `MetaServiceGroupStatus` |
| `pkg/storage/region_storage.go` | `RegionStorage` |
| `pkg/storage/region_storage_test.go` | `NewStorageEndpoint`, `RegionStorage` |
| `pkg/storage/storage.go` | `AffinityStorage`, `ConfigStorage`, `ExternalTSStorage`, `GCStateStorage`, `KeyspaceGroupStorage`, `KeyspaceStorage`, `MaintenanceStorage`, `MetaServiceGroupStorage`, `MetaStorage`, `MinResolvedTSStorage`, `RegionStorage`, `ReplicationStatusStorage`, `ResourceGroupStorage`, `RuleStorage`, `ServiceMiddlewareStorage`, `TSOStorage` |
| `pkg/storage/storage_test.go` | `RegionStorage` |
| `pkg/tso/allocator.go` | `TSOStorage` |
| `pkg/tso/keyspace_group_manager.go` | `KeyspaceGroup`, `KeyspaceGroupMember`, `NewStorageEndpoint`, `StorageEndpoint` |
| `pkg/tso/keyspace_group_manager_test.go` | `InitClusterID`, `KeyspaceGroup`, `KeyspaceGroupMember`, `MergeState`, `SplitState` |
| `pkg/tso/tso.go` | `TSOStorage` |
| `plugin/scheduler_example/evict_leader.go` | `ConfigStorage` |
| `server/api/service_gc_safepoint.go` | `ServiceSafePoint` |
| `server/apiv2/handlers/maintenance.go` | `MaintenanceLockName`, `MaintenanceTask` |
| `server/apiv2/handlers/meta_service_group.go` | `MetaServiceGroupStatus` |
| `server/apiv2/handlers/tso_keyspace_group.go` | `Basic`, `IsUserKindValid`, `KeyspaceGroup`, `KeyspaceGroupMember`, `MergeState`, `SplitState` |
| `server/config/persist_options.go` | `ConfigStorage` |
| `server/config/service_middleware_persist_options.go` | `ServiceMiddlewareStorage` |
| `server/gc_service.go` | `GCBarrier`, `GlobalGCBarrier` |
| `server/microservice_cleanup.go` | `KeyspaceGroup` |
| `server/microservice_cleanup_test.go` | `Basic`, `KeyspaceGroup`, `KeyspaceGroupMember`, `MergeState`, `SplitState`, `Standard` |
| `server/server.go` | `InitClusterID`, `MaxKVRangeLimit` |
| `tests/integrations/client/client_test.go` | `ServiceSafePoint` |
| `tests/integrations/client/http_client_test.go` | `ServiceSafePoint` |
| `tests/integrations/mcs/keyspace/tso_keyspace_group_test.go` | `KeyspaceGroup`, `KeyspaceGroupMember`, `Standard` |
| `tests/integrations/mcs/members/member_test.go` | `KeyspaceGroup`, `KeyspaceGroupMember`, `Standard` |
| `tests/integrations/mcs/scheduling/config_test.go` | `NewStorageEndpoint`, `StorageEndpoint` |
| `tests/integrations/mcs/tso/api_test.go` | `KeyspaceGroup` |
| `tests/integrations/mcs/tso/keyspace_group_manager_test.go` | `KeyspaceGroup`, `KeyspaceGroupMember`, `Standard`, `UserKind`, `UserKindCount` |
| `tests/integrations/mcs/tso/server_test.go` | `Basic`, `KeyspaceGroup`, `KeyspaceGroupMember` |
| `tests/integrations/tso/client_test.go` | `KeyspaceGroup`, `Standard` |
| `tests/server/api/service_gc_safepoint_test.go` | `ServiceSafePoint` |
| `tests/server/apiv2/handlers/maintenance_test.go` | `MaintenanceTask` |
| `tests/server/apiv2/handlers/meta_service_group_test.go` | `MetaServiceGroupStatus` |
| `tests/server/apiv2/handlers/testutil.go` | `KeyspaceGroup` |
| `tests/server/apiv2/handlers/tso_keyspace_group_test.go` | `KeyspaceGroup`, `KeyspaceGroupMember`, `Standard` |
| `tests/server/keyspace/keyspace_test.go` | `SpaceIDBase` |
| `tests/tso_cluster.go` | `KeyspaceGroupMember` |
| `tools/pd-ctl/tests/keyspace/keyspace_group_test.go` | `KeyspaceGroup`, `KeyspaceGroupMember`, `Standard` |
| `tools/pd-ctl/tests/safepoint/safepoint_test.go` | `ServiceSafePoint` |

## 相邻测试完整入口

以下 endpoint 全部测试文件及 storage 相邻契约测试逐项记录；断言意义见主表。
没有新建测试，也没有将测试声明当运行通过。无直接对应测试的项已经在主表注明后续补证据。

| 文件 | 顶层测试入口（不计 TestMain） |
| --- | --- |
| `pkg/storage/endpoint/cluster_id_test.go` | `TestInitClusterID` |
| `pkg/storage/endpoint/gc_state_batch_test.go` | `TestLoadGCSafePointPairs`, `TestLoadGCSafePointPairsBatchBounds`, `TestLoadGCSafePointPairsUnsupported`, `TestLoadGCSafePointPairsContext`, `TestLoadGCSafePointPairsInvalidResponse` |
| `pkg/storage/endpoint/gc_states_test.go` | `TestGCBarriersConversions`, `TestGlobalGCBarriersMarshalling`, `TestGCStateJSONUtil`, `TestGCStateTransactionACID`, `TestGCSafePoint`, `TestGCBarrier`, `TestGlobalGCBarrier`, `TestGCBarrierExpiring`, `TestTxnSafePoint`, `TestTiDBMinStartTS`, `TestDataPhysicalRepresentation` |
| `pkg/storage/endpoint/tso_keyspace_group_test.go` | `TestLoadKeyspaceGroupRevisionComparisonKeepsRequestBounded` |
| `pkg/storage/storage_test.go` | `TestBasic`, `TestLoadStores`, `TestStoreWeight`, `TestTryGetLocalRegionStorage`, `TestLoadRegions`, `TestLoadRegionsToCache`, `TestLoadRegionsExceedRangeLimit`, `TestTrySwitchRegionStorage` |
| `pkg/storage/storage_tso_test.go` | `TestSaveTimestampWithTimeout`, `TestSaveLoadTimestamp`, `TestTimestampTxn`, `TestSaveTimestampWithLeaderCheck` |
| `pkg/storage/storage_resource_group_test.go` | `TestResourceGroupStorage` |
| `pkg/storage/keyspace_test.go` | `TestSaveLoadKeyspace`, `TestLoadRangeKeyspaces`, `TestEncodeSpaceID` |
| `pkg/storage/meta_service_group_test.go` | `TestMetaServiceGroupStorage` |
| `pkg/storage/region_storage_test.go` | `TestRegionReload`, `TestRegionStorage` |

## 本轮验证与差异审查

- `rg -n '^(func |type |pub |    pub )' pkg/storage/endpoint pkg/storage/storage.go`：退出 0；已补读多行签名、接口、结构体、枚举常量及全部生产函数体。
- `git diff --name-only 5d5948052a9ee9e3fa7a09145fe2916a2912c223 -- pkg/storage/endpoint pkg/storage/storage.go`：退出 0、无输出；全部来源与规划基准相同。
- Python 只读清单核对：20 个生产文件的 SHA-256、132 个公开函数/方法声明和全部公开类型均在附录；87 个直接 import 消费文件已列入表。
- 路径检查：131 个显式仓库文件路径存在；拟建 endpoint manifest/lib 在前文明确标注；未引入指向已删除任务的链接。
- `git diff --check` 与 `git diff --cached --check`：退出 0；审查仅本证据文档与任务状态/删除，无代码、配置、manifest 或 lock 变更。
- 开始与结束逐项 SHA-256 对照 1101 个已跟踪 Go/模块文件，相同；Git 差异为空，未跟踪 Go/模块文件为 0。未调用 failpoint、Go、Cargo、生成/格式化/依赖命令。

本轮为文档事实清单，TDD/测试运行不适用；无 Cargo 槽位、无有效测试数量可报告。
批次 20 的文件、公开符号、值类型、key/value、事务、watch/lease、加密和缺口归属均已盘点；
仅该盘点完成，不表示任何后续实现或来源史诗完成。其他在途计划变更不纳入本提交。

## 任务 2：端点编码基础（批次 21，2026-10-23）

批次 20 提交 `b79e49d0a15fd0aae9a0053c9c16ecf52aad8e66` 的盘点及批次 19
准入已核验；来源 endpoint.go / util.go 相对规划基准无差异。以下为本轮实现与运行
证据，不改变上文任务 1 的历史审计结论。新 canonical crate 为
`astersql-pd-storage-endpoint`，默认 features，无新增 crate feature、外部依赖版本
或平行 MetaStore。根 manifest 仅注册该 member / workspace dependency，锁文件
仅增加该本地 package 及既有依赖引用。

### 实现与覆盖边界

- `pkg/storage/endpoint/endpoint_codec.rs`：StorageEndpoint 持有宿主传入的
  Arc<dyn MetaStore>，每次接收显式绝对 key，无全局 cluster ID 或私有 runtime。
  load 返回 Missing / Empty / Value，保留物理存在状态；load_json 将缺失/空字节
  返回 T::default，顶层 JSON null 同样返回默认值；坏数据为 JsonUnmarshal。
  save_json 先编码再保存，编码错误不覆写旧值，存储失败保留 StoreError。
- `protobuf` 是公开的持久化协议适配模块，prost 只在该边界；save 编码到 bytes
  再写，load 对缺失/空值返回 None，坏非空数据为 ProtoUnmarshal。领域 endpoint
  的 JSON / load API 不暴露生成 protobuf 类型。两类写入均使用 canonical Base，
  无 lease，不另建事务后端。
- `endpoint_codec_test.rs` 在 lib.rs 显式注册；endpoint_codec_memory 与
  endpoint_codec_etcd 共用 contract。真实 etcd 复用 ../kv/etcd_fixture_test.rs，
  fixture 使用随机 loopback 端口、独立目录、EOF 退出并等待子进程。
- 字节比对覆盖 ColorGroup、整数 1、字符串 str、null 与 RequestHeader。
  JSON 是显式 Serde 持久化模型，字段命名/空指针等 Go schema 映射仍由各业务模型
  适配；本任务不宣称任意 Go 类型与任意 Rust 类型（例如 bytes、浮点、自定义
  MarshalJSON）天然同构。完整业务 schema 的字节验证仍归各后续端点任务。
  prefix 分页、条件事务、业务算法、加密及服务启动不在此单键 load/save 验证故事内。

### 只读 Go 运行证据与精确输出

最初未找到端点 JSON 保存输出，因此记录过阻塞；继续只读排查后复用工具链的既有
示例与端点现有测试，未新增、修改或生成仓库 Go oracle。Go 1.26.8 标准库
encoding/json/example_test.go 的 ExampleMarshal 定义输入
ColorGroup{ID:1, Name:"Reds", Colors:["Crimson","Red","Ruby","Maroon"]}，直接
调用与 saveJSONInTxn 相同的 encoding/json.Marshal，并将返回 bytes 写至 stdout。
Go 示例测试框架实际核对 Output 后通过，而非仅抄源码推导预期。精确内容为：

```json
{"ID":1,"Name":"Reds","Colors":["Crimson","Red","Ruby","Maroon"]}
```

工具链示例文件 SHA-256：
`2745c3eac5bf5aac2d0913f65cb1535edd6358530d72ce99bd8df206c559e54e`。
已读示例和标准库测试入口，未发现 TestMain/生成执行；标准 go test 不运行
`go generate`，指定 Example 只执行既有函数，无 failpoint 或源码写入。

`TestGCStateJSONUtil` 现有测试经真实 etcd 写入并精确断言字节 `1`、`2`、
`"str"`、`0`、`null`，本轮实际通过；测试源包含读取 loadJSON、nullable 值断言。
其 ServiceSafePoint 场景只用 JSONEq，因此不把该 struct 场景当逐字节证据。
已审查该测试、TestMain、newEtcdStorageEndpoint、etcdutil/testutil 生命周期，
没有需要注入的 failpoint；GOFLAGS=-mod=readonly，输出仅临时 etcd 数据及测试缓存。
`TestUnmarshalNulls` 实际证明 Go null 不修改原始标量；本 helper 从零值开始，因此
对应 Rust 默认值。未执行完整 storage 包的 failpoint 测试，不以所选单测冒充整包通过。

protobuf 复用 `pkg/proto/wire-fixtures.json` 的 request_header，来源
`scripts/rust-proto-oracle/main.go`：ClusterId=42、SenderId=7、CallerId=oracle、
CallerComponent=pd；真实 Go 编码 hex 为
`082a10071a066f7261636c6522027064`。两个后端均比较 Rust 保存 bytes 与此既有输出，
再解码并比较所有字段；不要求新生成 Go oracle。

### TDD 与验证

- 失败命令 `cargo test --locked -p astersql-pd-storage-endpoint --lib endpoint_codec
  -- --nocapture`：退出 101，1 passed / 2 failed / 0 ignored。最小可编译接口将所有
  读视为 Missing；MemoryStore、etcd 均在写入空 bytes 后的 Empty 状态断言失败。
  初次编译缺少 serde derive 的辅助错误已修复，不将编译错误算作行为红灯。
- 审查追加顶层 null 标量断言后，同一命令再次退出 101，1 passed / 2 failed；
  两后端实际报 JsonUnmarshal(invalid type: null, expected i64)。随后修复默认值处理，
  未削弱或忽略原断言。
- 最终聚焦命令同上：退出 0，3 passed / 0 failed / 0 ignored；MemoryStore 与真实
  etcd 的完整 contract 均通过，纯值字节测试也被 endpoint_codec 过滤命中。
- `GOFLAGS=-mod=readonly go test encoding/json -run '^ExampleMarshal$' -count=1 -v`：
  退出 0，1 个 Example 通过。
- `GOFLAGS=-mod=readonly go test encoding/json -run '^TestUnmarshalNulls$' -count=1 -v`：
  退出 0，1 个测试通过。
- `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage/endpoint
  -run '^TestGCStateJSONUtil$' -count=1 -v`：退出 0，1 个顶层测试通过。
- 初增 crate 后 `cargo fmt --all`、`cargo check --workspace`：退出 0，同步锁文件，
  审查仅新增本地 package；后续均用 --locked。Rust 修改后先 fmt 再行为验证。
- 首轮 metadata / fmt-check / workspace check / workspace test 退出 0；workspace
  汇总 502 passed / 0 failed / 1 既有 ignored。null 修正后重新运行最终基线，见下方。

所有 Cargo 阶段在仓库根原子 mkdir 动态领取最先空闲的共享槽位；本轮实际全部领取
槽位 **1**，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划/任务/会话和执行 shell PID，以 EXIT/INT/TERM trap 清理自己的 owner
与锁目录，等待 Cargo 及 fixture 子进程退出后释放；缓存保留。所有 fixture Go build
继承 GOFLAGS=-mod=readonly；已审查既有 fixture 只构建已有源码，运行只写槽位数据。
未使用固定编号分配、--target-dir、failpoint 或 Go 生成/格式化/依赖整理入口。

最终基线（null 修正之后，各命令退出 0）：

| 命令 | 有效结果 |
| --- | --- |
| `cargo metadata --format-version 1 --no-deps` | endpoint target 已注册 |
| `cargo fmt --all -- --check` | 格式通过 |
| `cargo check --workspace --locked` | 全 workspace 编译检查通过 |
| `cargo test --workspace --locked` | 502 passed / 0 failed / 1 既有 ignored（含 doc-tests） |
| `git diff --check` | 通过 |

最终逐项 SHA-256 对照任务开始的 1101 个 Go/模块文件，内容全部相同；无未跟踪
Go/模块文件。Git Go 差异为空。只审查并提交 endpoint crate、最小 manifest/lock
连线、本证据节和编号任务清理；不修改只读 plan.md、来源主题及其他在途计划。
共享槽位锁已释放，缓存保留。批次 21 本验证故事已完成，不表示后续业务端点完成。

## 任务 3：集群标识初始化（批次 22，2026-10-23）

### 准入、Go 依据和兼容边界

批次 21 提交 `b3104ff54` 及前节红绿灯/502 项 workspace 通过证据已核验；
canonical endpoint crate 和共享 fixture 存在。本轮只实现 cluster ID 初始化和
MCS 只读等待。`cluster_id.go`、`cluster_id_test.go` 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

只读 Go 依据：`getClusterIDFromEtcd` 缺失返回 0，非空/空损坏值交由
`typeutil.BytesToUint64` 检查严格 8 字节；`InitClusterID` 先廉价读取非零 ID，
否则 `initOrGetClusterID` 使用 CreateRevision == 0 / Put / Else Get 原子事务。
失败分支要求一个响应、一个键值；竞争失败者解析同事务读取的胜出值，不重试写入。
已有零值仍占用键，因此候选不能覆写它。候选计算为 uint64 秒左移 32 位加随机
uint32，溢出保持 Go uint64 语义。时间和随机输入由 Rust 宿主提供。

`InitClusterIDForMs` 只读取，缺失/零/坏值/读错误均重试，间隔来自
`constant.RetryInterval`（500ms）。Go 使用全局 keypath 缓存且取消分支的命名 err
被内层 err 遮蔽，可能返回 nil；Rust 遵循本任务的无全局 ID、等待支持取消边界，
返回读取到的 u64 或 canonical `StoreError::Cancelled`，并可中断在途读。
这是显式的 Rust 取消结果，不宣称复现 Go 的 nil 返回或进程全局缓存副作用。
库不建立 runtime、后台任务或独立 MetaStore，调用方可丢弃初始化 future。

### 真实 Go 运行和字节证据

已审查现有 `TestInitClusterID` / TestMain / NewTestEtcdCluster / 连接清理入口；
目标测试不启用也不依赖 failpoint 注入。etcdutil 的可选注入站点不属于本测试
所需行为。只执行既有测试，未写 Go oracle；GOFLAGS 与命令均固定 readonly。
以下命令退出 0：

- `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage/endpoint -run
  '^TestInitClusterID$' -count=1 -v`：1 项通过，真实 etcd 验证从缺失到非零、重复
  初始化相同 ID、读取一致。未运行完整 endpoint/storage oracle：storage 相邻
  测试要求 failpoint，完整命令不符合本任务只读条件；不把所选单测当整包通过。
- `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/utils/typeutil -run
  '^(TestBytesToUint64|TestUint64ToBytes)$' -count=1 -v`：2 项通过。
  既有 `conversion_test.go` 实际调用与 cluster ID 相同的编码函数并精确断言
  输入 1000 输出 hex `00000000000003e8`，反向解码为 1000。
  Rust 两后端保存后与该 Go 输出比较，未以 Rust 生成值充当 Go 预期。
  候选溢出、错误长度和 MCS 的 Go 依据为源码推导，未伪称额外 Go 运行差分。

### 实现与 TDD

- `cluster_id.rs` 扩展 canonical `StorageEndpoint`：`get_cluster_id`、
  `init_cluster_id`、`wait_cluster_id`；`cluster_id_candidate` 为纯值计算。
  `ClusterIdError` 区分存储错误、错误长度、事务响应冲突。复用现有
  `CLUSTER_ID_PATH`、MetaStore 原子 raw transaction 与大端字节，无 protobuf
  泄漏或新依赖版本。manifest/lock 仅引用既有 keypath / tokio / tokio-util。
- 独立 `cluster_id_test.rs` 在 lib.rs 显式注册，MemoryStore 与真实 etcd 共用
  contract：缺失、Go 字节、读取、重复初始化不改变 entry revision、坏长度
  0/1/7/9、CAS 失败分支解码及不覆写坏值、既有零值和新建零候选、16 个初始化者
  同时竞争。并发阶段直接进入生产 CAS helper，确保竞争者确实提交条件事务，
  所有返回同一候选且持久化相同值；公开初始化入口另验证读及创建路径。
- MCS：缺失等待先轮询 30ms；写入坏值、零值分别轮询 550ms 仍不完成；写入 Go
  字节后 2 秒内得到 1000。只读等待不生成 ID；空键等待后取消在 200ms 内返回
  Cancelled 且键仍缺失；已有 ID 时预取消也返回 Cancelled。
- 真实断连时序：先保存 1000，fixture `stop` 确认停服，再轮询等待读 30ms 证明
  仍在途，取消后 200ms 内结束；同 WAL `start` 确认重启后 ID 与 Go 字节未变。
  无模拟核心事务、忽略测试或断连后盲目重试写。
- 初始可编译接口仅返回候选而未持久化。红灯命令
  `cargo test --locked -p astersql-pd-storage-endpoint --lib cluster_id -- --nocapture`
  退出 101：1 passed / 2 failed / 0 ignored；两个后端均在真实持久化字节断言
  失败，实际 `[]`，预期 `[0,0,0,0,0,0,3,232]`。
- 实现后同命令退出 0：原 3 项全部通过；保留原断言并补充实际 CAS 分支和断连
  取消检查后最终同命令退出 0，4 passed / 0 failed / 0 ignored。

首轮 workspace 回归发现测试装配问题（退出 101）：endpoint 的 7 项中 6 passed /
1 failed，失败在共享 fixture 启动阶段 `etcd fixture exited`，不是业务断言。
新旧测试模块分别 `#[path]` 引入同一个 helper，产生两套进程内 NEXT 计数器；
目录公式只有 PID 和 NEXT，因而同进程都从 `etcd-contract-<pid>-0` 开始。
另一个 fixture 的正常清理也会删除该目录。修复仅将现有 codec 测试的 fixture
模块可见性改为 `pub(super)`，新测试直接复用该模块，保持单一 BINARY/NEXT 和
原 helper 生命周期；没有复制或修改 kv helper、Go fixture 源码或其他业务行为。

### 最终验证与共享槽位

所有 Cargo 阶段从仓库根用原子 mkdir 依次动态领取首个可用锁，本轮均为槽位
**1**，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划、任务、会话和 shell PID；EXIT/INT/TERM trap 负责清理自己的 owner
与锁目录，阶段等待 Cargo 和 fixture 全部退出后释放，保留缓存。fixture 构建继承
GOFLAGS=-mod=readonly，仅构建既有 Go 源；运行仅写槽位内隔离目录。

下表是修复 fixture 模块复用之后的完整最终阶段；各命令退出码均为 **0**：

| 命令 | 有效结果 |
| --- | --- |
| `cargo fmt --all` | Rust 修改后先格式化；未改动其他 Rust 文件 |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib cluster_id -- --nocapture` | 4 passed / 0 failed / 0 ignored，包含两个后端与断连取消 |
| `cargo metadata --format-version 1 --no-deps` | canonical endpoint lib target 已注册 |
| `cargo fmt --all -- --check` | 格式通过 |
| `cargo check --workspace --locked` | workspace 检查通过；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 506 passed / 0 failed / 1 既有 ignored，含 doc-tests；endpoint 7 项全通过 |
| `git diff --check`、`git diff --cached --check` | 差异审查通过 |

初增依赖引用时 `cargo check --workspace` 退出 0，同步锁后已审查只新增两个既有
本地/已锁依赖引用；后续检查与测试全部使用 --locked。开始与结束逐项 SHA-256
核对 1101 个 Go/模块文件，全部相同；Go Git 差异为空，未跟踪 Go/模块文件为 0。
无 failpoint、Go 生成、Go 格式化、依赖整理或 Go 模块修改。

审查范围为新 cluster_id 实现/独立测试、codec store 的 crate 内可见性、现有
测试 fixture 的父模块可见性、lib 注册、endpoint manifest/lock、当前证据节和
任务文件清理。来源 plan.md、来源史诗及其他在途计划保持开始时原样。
槽位锁已释放，缓存保留。批次 22 此验证故事已完成，不扩大为服务启动或整个
endpoint Go 包迁移完成。


## 任务 4：集群元信息持久化（批次 23，2026-10-23）

### 准入、Go 行为与阻塞恢复

已核验批次 22 提交 `d874d1d3c44ef4f098d051e7e889ae799d9ce61f`、
前节聚焦 4 passed / workspace 506 passed、canonical endpoint 与 fixture 实物。
`meta.go` 相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223`
无差异；来源 plan.md、来源主题及其他计划不修改。

Go `StorageEndpoint.LoadMeta/SaveMeta` 通过 `keypath.ClusterPath` 读写
`metapb.Cluster`，`util.go` 的 loadProto 对缺失和空值返回 false，对损坏非空值
返回 false 与 ErrProtoUnmarshal；saveProto 使用 gogo proto.Marshal。
固定依赖 kvproto `v0.0.0-20261016101550-f8ad079c8e98` 的
`pkg/metapb/metapb.pb.go::Cluster` 定义 Id uint64 field 1、MaxPeerCount uint32
field 2；Unmarshal 检查 wire type、截断和溢出并跳过未知字段。这些边界是只读
源码依据，未冒充逐项 Go 运行差分。

初查现有 wire fixtures、rust-proto-oracle 固定映射和 kvproto 自身四个测试文件，
均无 Cluster 编码断言/输出，因此曾保留任务阻塞。恢复检查找到既有
`tests/server/cluster/cluster_test.go::TestLoadClusterInfo`：向真实 etcd 保存
`metapb.Cluster{Id:123}`（MaxPeerCount 为 0），然后重新 LoadClusterInfo 并断言
meta/stores/regions。通过正常 etcd HTTP gateway 观察其输出，已补齐所需真实字节。

### 只读 Go 运行及真实编码输出

先审查所选 TestBasic / TestLoadClusterInfo、TestMain/goleak、NewTestCluster、
config Generate、NewTestServer、RunInitialServers、默认 etcd storage、
SaveMeta/saveProto、etcdKVBase.Save、Destroy 调用链。所选场景不启用也不依赖
failpoint，运行只创建并清理临时 etcd/region 数据目录及日志，不写 Go/模块文件；
只执行现有测试，无 Go 工具新增、调试注入、生成器或 failpoint 启停。
整包 oracle 包含需注入的其他测试，未执行，所选测试不等于整包通过。

- `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^TestBasic$'
  -count=1 -v`：退出 0，1 passed，Cluster 缺失、保存、读取和相等断言通过；该测试
  没有输出字节，不单独作为字节证据。
- `GOFLAGS=-mod=readonly go test -mod=readonly ./tests/server/cluster -run
  '^TestLoadClusterInfo$' -count=1 -v`：退出 1，0 有效测试；Dashboard 缺少生成的
  assets/vfsgen 类型。未生成 Go 资产，改用仓库已有 without_dashboard 标签。
- `GOFLAGS=-mod=readonly go test -mod=readonly -tags without_dashboard
  ./tests/server/cluster -run '^TestLoadClusterInfo$' -count=1 -v`：退出 0，
  1 passed / 0 failed；单测 3.37s，包汇总 5.647s。

该命令由 Python subprocess 执行；观察线程从本次 stdout 发现随机 loopback URL，
每次调用仅 POST `/v3/kv/range`，body 为
`{"key":"L3BkLw==","range_end":"L3BkMA=="}`（`/pd/` 到 `/pd0`）。
按 `/pd/[0-9]+/raft` 过滤返回 key，base64 解码 value 并保留原始响应；
线程每轮间隔 2ms、请求超时 150ms，测试退出后停止并 join，没有修改服务或数据。
从 subprocess 启动（含编译）起 12.1962s 读取到：

```json
{
  "elapsed_seconds": 12.1962,
  "endpoint": "http://127.0.0.1:50371",
  "key": "/pd/7699712222286792390/raft",
  "value_hex": "087b",
  "raw_kv": {
    "key": "L3BkLzc2OTk3MTIyMjIyODY3OTIzOTAvcmFmdA==",
    "create_revision": "12",
    "mod_revision": "12",
    "version": "1",
    "value": "CHs="
  },
  "header": {
    "cluster_id": "3193401415776971262",
    "member_id": "14230389871356479017",
    "revision": "12",
    "raft_term": "2"
  }
}
```

输入来自上述既有 SaveMeta 调用；实际原始 value base64 `CHs=`、hex **087b**，
create/mod revision 均为 12，version 1。这是本轮真实 Go 编码输出，
不从 Rust 编码或 protobuf 字段号推导。Rust 两后端保存后直接与该字节比较，
并独立写入该 Go 输出验证读取。

### Rust 实现、TDD 与行为范围

`cluster_meta.rs` 在 canonical endpoint crate 扩展 `load_meta` / `save_meta`，
公开领域 `Cluster { id, max_peer_count }`；内部 WireCluster 与已有 protobuf codec
完成转换，生成协议消息不进入公开接口。路径复用调用者传入的 ClusterPaths，
不引入全局 ID、第二套 MetaStore、依赖版本或配置开关。存储错误与 protobuf 错误
沿用 EndpointError。全零 Cluster 保存为空 wire，随后读为 None，保持 Go 空值语义；
不强制 meta.id 与 key 的 cluster ID 相等（现有 Go 测试也允许不同）。

独立 `cluster_meta_test.rs` 在 lib.rs 显式注册。MemoryStore 与真实 etcd 共用 contract，
覆盖缺失、空值、真实 Go 字节双向读取、完整字段往返、u64/u32 最大值、ID 为零但
peer count 非零、路径隔离、未知字段、坏 tag/截断/错误 wire type、覆盖写与全零值。
额外字段和畸形输入基于上述 Go 源码，未声称新增 Go 编码样本。
真实 etcd 最后再保存 123，收到 stop/stopped 确认后 start/restarted 同 WAL 重启，
重新连接并验证领域值和物理字节仍为 Go 输出，无进程或 fixture 遗留。

初始 save_meta 为可编译但不写入的最小接口。Rust 修改后先 `cargo fmt --all`：

- 红灯 `cargo test --locked -p astersql-pd-storage-endpoint --lib cluster_meta
  -- --nocapture`：退出 101，0 passed / 2 failed / 0 ignored；两个后端均在真实
  byte 断言失败，实际 `[]`、预期 `[8,123]`，不是缺符号或环境失败。
- 最小保存实现后，同命令退出 0，2 passed / 0 failed / 0 ignored，原断言未削弱。

### 最终验证、共享槽位与差异核验

每个 Cargo 阶段从仓库根以 mkdir 原子动态领取第一个可用槽位，本轮三阶段均
实际领取 **1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁 owner 记录计划、任务/批次、会话 cluster-meta-recovery 与 shell PID；
EXIT/INT/TERM trap 清理本人的 owner 与锁目录。命令同步等待所有 Cargo/fixture
子进程退出后释放锁，缓存保留，无其他锁或缓存修改。所有子进程继承
GOFLAGS=-mod=readonly；现有 fixture Go build 只构建既有源码。proto 构建脚本
有解压 Go 源码路径，本轮复用已有槽位构建产物，未触发该脚本，不新增该类文件。

Rust 实现完成后的最终阶段命令及结果：

| 命令 | 退出码与有效结果 |
| --- | --- |
| `cargo fmt --all` | 0；仅本任务 Rust 文件变化 |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib cluster_meta -- --nocapture` | 0；2 passed / 0 failed / 0 ignored，两个后端均命中 |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint lib 已注册 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；只有既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；508 passed / 0 failed / 1 既有 ignored，含 doc-tests；endpoint 9 项全通过 |
| `git diff --check`、`git diff --cached --check` | 0 |

任务开始/恢复前与最终对照 1101 个已跟踪 Go/模块文件 SHA-256，全相同；Go Git
差异为空，未跟踪 Go/模块文件 0。未运行 failpoint、Go 生成器/格式化/依赖整理。
manifest、Cargo.lock 无修改，workspace 格式化未改动其他 Rust 文件。
审查仅 cluster_meta.rs、cluster_meta_test.rs、lib 注册、本证据节及任务文件清理。
批次 23 当前验证故事已完成；不扩大为整个 endpoint 包或服务启动完成。


## 任务 5：单个存储节点元信息（批次 24，2026-10-23）

### 准入与只读 Go 依据

核验前序提交 `6ff1c2b18`、任务 4 的实物及上节证据：聚焦两个后端通过，
workspace 508 passed / 0 failed / 1 既有 ignored，批次 23 已满足。
`meta.go` 相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223`
无差异。已读根 AGENTS、Copilot、ADR-001、porting-guide 与 canonical core/endpoint。
不修改来源主题、plan.md 或后续任务。

Go `meta.go::{LoadStoreMeta,SaveStoreMeta,DeleteStoreMeta}` 分别复用 loadProto、
saveProto、Remove；key 是 `StorePath(storeID)`，保存/删除取 metadata ID；
删除不触及 leader/region weight。`util.go::loadProto` 缺失/空值返回 false，
坏非空 protobuf 返回 false+ErrProtoUnmarshal；保存不验证地址/状态/版本。
点读不执行 LoadStores 的 Offline/Tombstone -> Removing/Removed 映射。
固定 kvproto `v0.0.0-20261016101550-f8ad079c8e98` 的 metapb.pb.go 定义
Store 的 14 个字段，含有符号 int64 时间、int32 枚举和 optional range（uint32 min/max）。
未知 enum 数值保留，未知字段跳过；这些边界属于只读源码依据。

复用任务 4 已审查的 TestMain、NewTestCluster、RunInitialServers、默认 etcd storage、
SaveStoreMeta/saveProto、etcdKVBase.Save、Destroy 调用链，并核对当前选中测试。
所选 TestBasic、TestLoadClusterInfo 不启用、不依赖 failpoint；运行仅写临时数据/日志，
未运行生成器、Go 格式化、依赖整理或 failpoint 启停。整包 oracle 包含需注入的场景，
本轮不裸跑整包，不将所选测试称为整包通过。

| 只读 Go 命令 | 退出码、有效结果 |
| --- | --- |
| `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^TestBasic$' -count=1 -v` | 0；1 passed；Store 缺失、保存、加载与字段相等 |
| `GOFLAGS=-mod=readonly go test -mod=readonly -tags without_dashboard ./tests/server/cluster -run '^TestLoadClusterInfo$' -count=1 -v` | 0；1 passed；2.31s，包 4.572s；保存十个 Store 并重载 |

### 实际 Go 编码输出

TestLoadClusterInfo 现有源码逐个保存 `metapb.Store{Id:uint64(i)}`，i=0..9。
Python 观察线程从测试 stdout 获取随机 loopback URL，每轮间隔 2ms，以 150ms
超时只调用 `/v3/kv/range`，body 为
`{"key":"L3BkLw==","range_end":"L3BkMA=="}`，筛选 `/pd/[0-9]+/raft/s/[0-9]{20}`。
不新增 Go 工具、不改测试、不注入调试分支、不写 etcd；进程结束后停止并 join 线程。
从 subprocess 启动（含编译）计时，取得如下原始响应样本：

```json
{
  "elapsed_seconds": 10.5487,
  "endpoint": "http://127.0.0.1:53145",
  "key": "/pd/7699713754536276199/raft/s/00000000000000000001",
  "value_hex": "0801",
  "raw_kv": {
    "key": "L3BkLzc2OTk3MTM3NTQ1MzYyNzYxOTkvcmFmdC9zLzAwMDAwMDAwMDAwMDAwMDAwMDAx",
    "create_revision": "14",
    "mod_revision": "14",
    "version": "1",
    "value": "CAE="
  },
  "header": {
    "cluster_id": "14839095727612203604",
    "member_id": "17920083281916526640",
    "revision": "14",
    "raft_term": "2"
  }
}
```

十个 Store 输出（均为该 Go 测试实际观察值）：

| ID | hex | create/mod revision | version |
| --- | --- | --- | --- |
| 0 | `(empty)` | 13/13 | 1 |
| 1 | `0801` | 14/14 | 1 |
| 2 | `0802` | 15/15 | 1 |
| 3 | `0803` | 16/16 | 1 |
| 4 | `0804` | 17/17 | 1 |
| 5 | `0805` | 18/18 | 1 |
| 6 | `0806` | 19/19 | 1 |
| 7 | `0807` | 20/20 | 1 |
| 8 | `0808` | 21/21 | 1 |
| 9 | `0809` | 22/22 | 1 |

Rust 两后端保存 ID=1 后逐字节断言 `0801`，并独立写入真实 Go 输出再读取；
全零 Store 为空值也与上述 Go ID=0 输出一致。完整字段、枚举和畸形字节用例依据
Go schema/源码验证 Rust 行为，未将它们宣称为额外 Go 全字段编码差分。

### 实现与 TDD

`store_meta.rs` 扩展 canonical StorageEndpoint 的 load_store_meta、save_store_meta、
delete_store_meta；公开 API 复用 `core::StoreMeta`，私有 wire 模块封装 metapb schema，
不引入协议类型到领域、不复制 MetaStore、不设全局 cluster ID。
已知全部字段显式双向转换，labels 保持顺序/重复/空值，未知枚举保留，optional range
区分缺失与显式 [0,0]，不修正 min>max 或生命周期状态，时间保留有符号边界。
沿用 EndpointError 区分存储及 protobuf 错误。删除只移除指定 metadata key。
只添加已有 workspace core 依赖；Cargo.lock 仅新增 endpoint -> core 一项。

独立 store_meta_test.rs 在 lib.rs 显式注册；store_meta_memory、store_meta_etcd
运行同一真实存储 contract，涵盖缺失/空值、Go 字节双向读取、全部字段往返、
u64/i64/u32 边界、未知及各已知状态、range 缺失/显式零值、重复/空 labels、
覆盖写、未知字段、坏 tag/wire type/截断/坏嵌套消息、幂等删除、权重及其他 ID 保留、
cluster 路径隔离、全零 Store。真实 etcd 额外保存完整值，收到 stop/stopped 后
start/restarted，以同 WAL 重启并重连读取，再删除确认缺失。

初始 save_store_meta 是可编译但不写数据的最小接口，修改 Rust 后先格式化：

- `cargo fmt --all`：退出 0。
- 新增 core 依赖后 `cargo check --workspace`：退出 0，同步 lock 并审查只有上述一项。
- 红灯 `cargo test --locked -p astersql-pd-storage-endpoint --lib store_meta -- --nocapture`：
  退出 101，0 passed / 2 failed / 0 ignored；两个后端在物理字节断言实际 `[]`、
  期望 `[8,1]` 失败，不是编译或环境失败。
- 最小保存实现后先 `cargo fmt --all`（退出 0），同一聚焦命令退出 0，
  2 passed / 0 failed / 0 ignored，5.85s；原断言未削弱。

### 最终验证与共享槽位

本轮每个 Cargo 阶段均从根目录依次原子 mkdir 动态领取第一个可用槽位，实际为 **1**；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan、task=5、batch=24、session=store-meta 与 shell PID；EXIT/INT/TERM
trap 清理自己的 owner/锁，全部同步 Cargo 和 fixture 子进程退出后才释放，缓存保留。
所有 Cargo 及 fixture 子进程继承该路径与 GOFLAGS=-mod=readonly；fixture 仅 go build
既有源码。复用已有 proto 缓存，未触发解压 Go 源码的 proto build.rs。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint 注册不变 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；510 passed / 0 failed / 1 既有 ignored，含 doc-tests；endpoint 11 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

最终核对开始时记录的 1101 个已跟踪 Go/模块文件 SHA-256，全相同；Go Git 差异为空，
未跟踪 Go/模块文件为 0。workspace 格式化没有修改本任务外 Rust 文件。
审查差异仅 store_meta.rs、独立测试、lib 注册、endpoint core 依赖和单项 lock 更新、
本证据节及任务文件清理；已有其他计划改动保持原样。Cargo 槽位锁已释放，缓存保留。
批次 24 当前验证故事已完成，不将其扩大为 LoadStores、权重 API 或整包迁移完成。

## 批次 25：存储节点权重与分页加载（任务 6）

准入：前序批次 24 提交 `c9312be15` 与本文对应证据完整，单 Store
读写/完整字段适配已验证；前序任务文件已在该提交删除。当前 meta.go
相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。
无更深层 AGENTS.md，已遵守 ADR-001、porting-guide 及 Go 只读硬约束。
本批次不修改任何计划总纲、来源主题、Go 源码或模块文件。

### Go 依据与实际只读验证

`pkg/storage/endpoint/meta.go` 的 SaveStoreWeight 以
`strconv.FormatFloat(value, 'f', -1, 64)` 编码，先保存 leader，再保存 region；
第一次失败即返回，第二次失败保留第一次写入。LoadStores 使用 `[StorePath(0),
StorePath(MaxUint64))`、100 条一页，下一页起点是最后解码的 metadata ID + 1。
每条直接 Unmarshal（空 protobuf 得零值 Store），Offline -> Removing，
Tombstone -> Removed，其他状态保留；再顺序读取 leader/region，空值或缺失均为 1，
非法浮点返回 parse float 错误，之前已交付的回调保留。

已审查 `storage_test.go` TestMain、mustSaveStores、TestLoadStores、
TestStoreWeight、newMemoryBackend、MemoryKV Save/LoadRange 和 endpoint 调用链：
所选测试不启用、不依赖 failpoint，不运行生成/格式化/依赖整理，不写 Go 文件。
整包包含 failpoint 用例，未将所选测试声称为整包 oracle。

| 确切命令 | 退出码、有效结果 |
| --- | --- |
| `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^(TestLoadStores\|TestStoreWeight)$' -count=1 -v` | 0；2 passed；10 个 Store 加载、3 个 Store 默认和显式权重断言 |
| `GOFLAGS=-mod=readonly go test strconv -run '^(TestFtoa\|TestAtof)$' -count=1 -v` | 0；Go 1.26.8 仅命中 TestAtof，1 passed；没有把未命中 Ftoa 算作成功 |
| `GOFLAGS=-mod=readonly go test internal/strconv -run '^(TestFtoa\|TestAtof)$' -count=1 -v` | 0；Go 1.26.8 数值测试位于 internal/strconv，2 passed |

编码依据复用批次 24 实测 Go Store ID=0..9 protobuf 字节；本任务没有新增协议
schema。权重编码使用已存在的 Go 1.26.8
`src/internal/strconv/ftoa_test.go` TestFtoa 真实执行结果：原测试逐项比较
FormatFloat 与已有字符串，再独立执行 Rust 保存后的物理字节断言：

| Go 输入 float64 | Go format/precision | Go 实际输出，与 Rust 比对一致 |
| --- | --- | --- |
| `1e23` | `'f', -1` | `100000000000000000000000` |
| `below1e23 = 99999999999999974834176` | `'f', -1` | `99999999999999970000000` |
| `above1e23 = 100000000000000008388608` | `'f', -1` | `100000000000000010000000` |
| `498484681984085570` | `'f', -1` | `498484681984085570` |

Go TestStoreWeight 的实际断言确认 2/3、0.2/0.3 和默认 1 的数值；
小数物理字节、负零/NaN/Inf 和更多 hex/下溢/舍入边界的 Rust 断言亦已执行。
没有把源码推导或 Rust 自产值称作额外 Go 动态输出。
另在 `/tmp` 运行既有 Go 1.24.3 stdlib 的
`GOTOOLCHAIN=local GO111MODULE=off go test strconv -run '^(TestFtoa|TestAtof)$' -count=1 -v`
退出 0，2 passed，仅作辅助；最初在仓库运行该旧版本被 go.mod 的版本门拒绝，
未改模块、未据此降低验收，最终采用上述当前 1.26.8 internal/strconv 证据。

### 实现、红灯与绿灯

canonical StorageEndpoint 新增 `load_stores`、`save_store_weight`；
复用 core::StoreInfo/StoreMeta 和既有 wire 转换。store_meta.rs 仅增加 crate 内
`decode_store` 供 range 读取复用（保持 point-read 空值语义）；EndpointError
增加 ParseFloat 分类，存储错误与 protobuf 错误保持原分类。
默认权重不持久化，范围排除 MAX ID，回调逐条生效，不收集全部结果才交付。
保存是两次独立事务；两后端均断言 region revision = leader revision + 1。
浮点遵循 Go 对无符号 NaN、带符号 Inf/Infinity、Go 数字下划线、hex p 指数、
溢出错误/下溢零的规则；hex 构建有效位后一次 ties-to-even 舍入，含 subnormal。
未新增依赖、feature、crate、全局状态或 runtime。

独立 `store_weights_test.rs` 在 lib.rs 显式注册；memory/etcd 入口执行同一 contract，
逆序插入有间隔的 205 个 Store，跨三页有序加载，验证 MAX 排除、状态映射、
原 metadata 不变、默认/显式权重、空值、错误前回调、region 非法值、畸形 protobuf、
空 protobuf 和 cluster 隔离。额外 float_boundaries 覆盖正常/次正规、舍入、
巨大指数零、非法语法和真实 Go 固定小数输出。

真实 etcd 还使用测试内 TCP 代理：leader 请求完整转发并收到提交响应；
后续 region 请求到达代理时，在完整键名被识别且该请求尚未完整转发之前关闭连接。
代理保留监听、不允许重连到上游，500ms 客户端期限后保存返回 Store 错误；
oneshot 确认故障边界确实被到达。取消并 join 代理后，直连同一 etcd 确认终态
leader=`2.5`、region 缺失。未模拟事务结果、未替换核心存储。

- 修改 Rust 后首先 `cargo fmt --all`，退出 0。
- 最小可编译空加载接口的红灯：
  `cargo test --locked -p astersql-pd-storage-endpoint --lib store_weights -- --nocapture`
  退出 101；0 passed / 2 failed / 0 ignored；两个真实后端均在条数断言
  actual=0、expected=205 失败，5.95s，非编译/环境错误。
- 最小实现同一命令绿灯：退出 0，2 passed / 0 failed / 0 ignored，6.72s。
- 扩充浮点边界与 TCP 部分写入后，先格式化，再执行同一聚焦命令：
  退出 0；3 passed / 0 failed / 0 ignored，4.86s；原断言未削弱。

### 最终验证、共享槽位与差异审查

所有 Cargo 阶段均从仓库根依次原子 mkdir 动态领取第一个可用槽位，实际均为 **1**；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁 owner 记录计划、task=6、batch=25、session=store-weights 与持有 shell PID；
验证包装器设置绝对路径及 GOFLAGS=-mod=readonly，EXIT/INT/TERM trap 清理自己的锁。
同步等待 Cargo、fixture 与代理子进程全部结束再释放，缓存保留。复用 proto 构建缓存，
没有运行解压/生成 Go 源码的 proto build.rs，也没有改动任何 Cargo manifest/lock。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint target 保持注册 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；513 passed / 0 failed / 1 既有 ignored，含 doc-tests；endpoint 14 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

最终核对开始时记录的 1101 个已跟踪 Go/模块文件 SHA-256，全相同；Go Git 差异为空，
未跟踪 Go/模块文件为 0。workspace fmt 未修改本任务外 Rust 文件。
审查范围仅 store_weights.rs/独立测试、lib 注册、现有 store_meta 的内部解码复用、
EndpointError 分类、本证据节及任务文件清理；用户已有其他计划修改保持原样。
共享槽位锁已释放，缓存保留。批次 25 当前验证故事已完成。

## 批次 26：单个区域元信息（任务 7，2026-10-23）

### 准入和只读 Go 证据

前序批次 25 提交 `cf930e6de`、store_weights 产物及本文证据齐全：
聚焦 3 passed，workspace 513 passed / 0 failed / 1 既有 ignored。
据此准入批次 26。meta.go 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异；无更深层
AGENTS.md，已读 ADR-001、porting-guide、Copilot 与 canonical core/endpoint。
本任务保持计划总纲和来源主题只读。

Go `meta.go::{LoadRegion,SaveRegion,DeleteRegion}` 使用 RegionPath(id)。
LoadRegion 缺失/空值返回 false，无后端错误时无错误；非空坏 protobuf 返回
**true + ErrProtoUnmarshal**，解码成功后调用 DecryptRegion。
SaveRegion 先 EncryptRegion，再 Marshal 和 Save；DeleteRegion 只 Remove
传入 metadata 的 ID 对应键。`encryption/region_crypter.go` 明确拒绝已经有
EncryptionMeta 的保存输入，无 manager 时不加密；读取有 EncryptionMeta 而无
manager 时返回解密错误，空的非 nil metadata 也不例外。
固定 kvproto `v0.0.0-20261016101550-f8ad079c8e98` 的 metapb.pb.go 为字段依据：
Region 字段 1–8，epoch conf/version，peer 的 id/store/role/witness；
未知 role 数值保留、未知字段跳过，默认 epoch 缺失与显式零值不同。

已审查 TestMain、WaitForEtcdConnections、TestBasic、memory backend、
MemoryKV 点读/保存/删除及 encryption 调用链。所选 TestBasic 不执行 range
failpoint、不启用或依赖注入；现有 rust-proto-oracle 仅构造消息、调用 gogo
proto.Marshal 并输出 stdout，不修改源码或模块文件。因此执行：

| 只读命令 | 退出码与结果 |
| --- | --- |
| `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^TestBasic$' -count=1 -v` | 0；1 passed；Region 缺失、保存、读取相等、删除后缺失 |
| `GOFLAGS=-mod=readonly go run -mod=readonly ./scripts/rust-proto-oracle` | 0；7 个真实序列化样本，与已有 wire-fixtures.json 一致 |

整包 oracle 有依赖 failpoint 的场景，本任务未裸跑整包，也不声称整包 Go 通过。
本轮复用已有 Go 工具，没有编写、生成或修改 Go 文件。

### 实际 Go 字节和 Rust 实现

Go 工具既有 `region_heartbeat` 输入中 Region 为
`metapb.Region{Id:99,StartKey:[]byte{0,1},EndKey:[]byte{255}}`。
本轮 stdout 实际完整 hex：
`0a10082a10071a066f7261636c652202706412090863120200011a01ff1a040803100430d209`。
其 field 2 长度为 9，原始子消息 hex **0863120200011a01ff**。
Rust 测试直接从已有 JSON fixture 的原始字节切出该子消息，检查外围 tag/length，
保存后逐字节比较，并独立写入这些 Go 字节验证读取；没有用 Rust 重编码值
构造预期。其他完整字段和畸形输入依据 Go schema/源码，不冒称额外 Go 运行差分。

canonical endpoint 新增 region_meta.rs 的 load_region、save_region、delete_region，
公开接口复用 core::RegionMeta；内部 wire adapter 隔离 protobuf，并完整映射字段。
RegionLoadError 保留 found 和 source：后端读错 false，坏非空 wire/加密 metadata
错误 true。RegionMetaError 区分 endpoint codec/后端错误与明确的加密拒绝。
保存拒绝带加密 metadata 输入，且在写入前返回，原物理字节保留。
不新增依赖、manifest、lock、feature、runtime 或全局 cluster ID。

独立 region_meta_test.rs 在 lib.rs 显式注册；region_meta_memory / region_meta_etcd
共用真实后端 contract，覆盖缺失/空值、Go 字节双向读取、完整字段往返、u64 边界、
全部 peer role 与未知负值/最大值、重复及零值 peer、epoch 缺失/显式零、原始二进制
反向范围、flashback、未知字段、畸形 wire/嵌套消息、空及非空加密元信息拒绝、
拒绝保存不改数据、覆盖写、幂等删除及其他 ID/cluster 隔离、全零 metadata。
etcd 额外保存完整值，在 stop/stopped、start/restarted 确认后重连，同 WAL 读取并
删除；随后 drop fixture 完成关闭并等待子进程，再发点读触发真实后端失败，
断言 found=false、Store 错误，不模拟存储返回结果。

### TDD 和共享槽位

先用可编译的空保存接口运行同一行为测试。每次 Rust 修改后先 `cargo fmt --all`，
退出 0；第一次聚焦命令
`cargo test --locked -p astersql-pd-storage-endpoint --lib region_meta -- --nocapture`
退出 **101**，0 passed / 2 failed / 0 ignored，4.77s；两个后端物理字节断言
实际 `[]`，预期 `[8,99,18,2,0,1,26,1,255]`，非缺符号或环境红灯。
最小真实保存实现后原断言未削弱，同命令退出 **0**，2 passed / 0 failed /
0 ignored，5.19s；包含真实重启与断连终态。

每个 Cargo 阶段从仓库根依次原子 mkdir 动态领取第一个空闲共享槽位，实际均为
**1**，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、task=7、batch=26、session=01a16709-region-meta 与 shell PID；
包装器 EXIT/INT/TERM trap 清理自己的 owner 和锁，所有 Cargo/fixture 子进程
同步结束后才释放，保留缓存。所有子进程继承该路径和 GOFLAGS=-mod=readonly。
既有 fixture 仅 build 既有 Go 源码；本轮复用 proto 缓存，未触发会解压 Go 源码的
proto build.rs，也未运行 failpoint、Go 生成器、格式化或依赖整理。

### 最终验证和差异审查

| 最终命令 | 退出码、有效结果 |
| --- | --- |
| `cargo metadata --format-version 1 --no-deps` | 0；endpoint canonical lib 注册保持一致 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；515 passed / 0 failed / 1 既有 ignored，83 个 target 汇总含 doc-tests；endpoint 16 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

最终核对开始时记录的 1101 个已跟踪 Go/模块文件 SHA-256 全相同，Go Git 差异为空，
未跟踪 Go/模块文件 0；Cargo manifest/lock 无变化，workspace fmt 未改其他 Rust。
审查仅 region_meta.rs、独立测试、lib 注册、本证据节及编号任务文件清理。
用户已有其他计划改动保持原样。Cargo/fixture 子进程全部结束，共享锁已释放，
缓存保留。批次 26 当前 Region 单键验证故事已完成；不包含范围分页或加密引擎。

## 批次 27：区域分页恢复（任务 8，2026-10-23）

### 准入与 Go 依据

批次 26 提交 `8536a8ec8`、region_meta 产物和本文对应证据齐全：聚焦
2 passed、workspace 515 passed / 0 failed / 1 既有 ignored，据此准入。
已读 ADR-001、porting-guide、Copilot、canonical endpoint/core 与 Go 相邻测试；
无更深 AGENTS.md。meta.go 对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223`
无变化。总纲和来源主题计划保持只读。

Go `StorageEndpoint.LoadRegions`：初始 limit=10000，读取失败依次减为
5000/2500/1250/625/312/156，下一次降到 78 时返回最后的读取错误；
所有重试保留 start cursor。每次成功读取后检查 ctx，再逐条 Unmarshal、Decrypt；
空字节在 range 中解码为零值 Region，与点读不同。cursor=metadata.ID+1 在回调和
重叠删除之前更新，使用 uint64 回绕。回调生成 Storage 来源、无 leader 的 RegionInfo；
只删除回调返回项。取消不打断已交付页；坏 wire/解密失败不推进当前项游标，
删除失败保留已推进游标。end=RegionPath(MaxUint64)，最大 ID 键不参与范围。
游标属于 endpoint，重复调用延续恢复进度；新 endpoint 从零开始。

已审查 `TestLoadRegions`、其 TestMain/WaitForEtcdConnections、memory backend、
MemoryKV range 和回调调用链；所选测试不启用或依赖 failpoint，不写 Go 源码/模块。
实际命令 `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^TestLoadRegions$' -count=1 -v`
退出 0，1 passed，固定 10 个区域恢复相等。`TestRegionReload` 与
`TestLoadRegionsExceedRangeLimit` 依赖注入，本轮不执行，不声称取得其运行差分。
整包 oracle 同样未裸跑；取消/缩页语义依据源码和 Rust 真实后端验证分别标明。
字节证据复用前序实际 Go 输出及已有 wire-fixtures.json 的 region_heartbeat
原始子消息 `0863120200011a01ff`，测试从原 JSON 字节切出后直接写入后端并恢复，
没有以 Rust 重编码结果构造预期。其他 ID wire 是依据 protobuf schema 的测试输入，
不声称新增 Go 字节运行差分。

### 实现与行为验证

新增 canonical `region_scan.rs::{load_regions,next_region_id,RegionScanError}`，
endpoint 仅增加一个实例游标字段；用 `&mut self` 保证扫描状态独占。
region_meta 内部共享 decode_region，复用现有 codec 和显式加密拒绝。
保持领域 RegionInfo 边界、宿主取消令牌，无新依赖/feature/runtime/全局状态。
独立 region_scan_test.rs 在 lib.rs 显式注册。

`region_scan_memory` / `region_scan_etcd` 共用同一真实存储 contract：Go 字节读取、
空 wire 零值、10001 条跨 100 和 10000 边界、回调取消后完成整页、下页取消不交付、
恢复游标继续、重叠区域物理删除、MAX 排除、坏 wire 失败后修复续读、预先取消。

`region_scan_etcd_adaptive_and_io_failure` 保存 220 条各含 24000 字节 start key
的真实区域。直接发 limit=10000 的真实读确认超过 4 MiB 响应导致错误，再使用
load_regions 自动缩页，完整恢复 ID 1..220 与全部长 key，终态 cursor=221。
未模拟核心存储结果。另一 cluster 保存 ID=1，扫描回调内关闭并等待真实 fixture
进程，然后返回该区域作为 overlap：删除返回 Store 错误，cursor 已为 2。
随后对已关闭服务扫描，所有缩页重试仍失败，返回 Store 错误且 cursor=2、无回调。
故障发生于实际交付之后、删除之前；fixture 子进程退出后才进行失败读取断言。

第一次接口编译错误（Mutation::put 参数借用）仅作辅助修正，不计行为红灯。
修复测试接口后，最小空扫描实现执行
`cargo test --locked -p astersql-pd-storage-endpoint --lib region_scan -- --nocapture`：
退出 101，0 passed / 2 failed / 0 ignored，4.77s；两后端均在实际交付条数
actual=0、expected=1 断言失败。真实实现后相同命令退出 0，2 passed，3.55s。
加入真实超限与断连场景后同命令退出 0，3 passed / 0 failed / 0 ignored，5.73s，
未削弱原断言。每次 Rust 修改后先 `cargo fmt --all`，均退出 0。

### 共享槽位及最终基线

每阶段从仓库根依次原子 mkdir 动态领取首个空槽，实际均为 **1**；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、task=8、batch=27、session=region-scan 和持有 shell PID；
包装器 EXIT/INT/TERM trap 清理自己的 owner/锁。所有 Cargo 与 fixture 子进程
同步结束才释放，缓存保留；子进程继承绝对 target 和 GOFLAGS=-mod=readonly。
复用 proto 缓存，没有触发会解压 Go 源码的 proto build.rs；fixture 仅编译已有
Go 工具，未运行生成器、failpoint、Go 格式化或依赖整理。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint 保持注册 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 个 target 汇总含 doc-tests，518 passed / 0 failed / 1 既有 ignored；endpoint 19 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

核对开始时记录的 1101 个已跟踪 Go/模块文件 SHA-256，全相同；Go Git 差异为空，
未跟踪 Go/模块文件为 0。Cargo manifest/lock 无变化；workspace fmt 未改其他 Rust。
已审查仅本任务 region_scan 实现/测试、endpoint 实例游标、内部 decode 复用、模块
注册及本证据节，用户已有其他计划变更保留。全部 Cargo/fixture 子进程结束，
共享锁已释放，缓存保留。批次 27 当前分页恢复验证故事已完成。

## 批次 28：普通与中间件配置端点（任务 9，2026-10-23）

### 准入、Go 依据与范围

批次 27 提交 `ac0b6f3d4`、region_scan 产物及本文对应完成证据齐全：聚焦
3 passed、workspace 518 passed / 0 failed / 1 既有 ignored，满足本批准入。
已读执行技能、ADR-001、porting-guide、Copilot 和 canonical endpoint；
`rg --files -g AGENTS.md pkg docs` 无更深指令。config.go/service_middleware.go
对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。
计划总纲和来源主题保持只读。

Go `LoadConfig` / `LoadServiceMiddlewareConfig` 各自单次 Load：缺失或空字节
返回 false，后端错误直接返回；非空值调用 encoding/json.Unmarshal，成功 true，
坏 JSON 返回 false + ErrJSONUnmarshal。Save 方法复用 saveJSON，先编码，成功后
覆盖单键，不附 lease；编码失败不得覆盖旧字节。路径为 /pd/C/config 和
/pd/C/service_middleware，复用现有 ClusterPaths，隔离 cluster 与两个配置种类。
这些语义依据源码；没有将源码推导冒充新增 Go 场景的运行差分。

本任务只提供泛型持久化适配，配置 schema 由调用者提供；不迁移完整 Config、
嵌套调度类型、server/config 默认值/校验或 PersistOptions。Rust API 返回
Result<Option<T>>：None 对应 Go found=false，Some(model) 对应 found=true。
它解码一个新模型，并不模拟 Go 对已有非零对象的原地修改；JSON null 使用
T::default（模型须提供 Go 零值），Option<T> 则为 Some(None)。字段缺省、nullable、
大小写兼容和自定义编码仍由具体 schema adapter 负责，不能据此宣称任意 Serde
类型与任意 Go 类型天然同构。测试 ColorGroup 的对象解码显式要求 map，因为
Serde 的 struct derive 默认也接受 positional array，而 Go struct 不接受数组。
未使用 raw JSON 透传冒充完整配置类型迁移。

### 只读 Go 字节证据

复用批次 21 已保存的 Go 1.26.8 encoding/json ExampleMarshal 真实输出：
输入 ColorGroup{ID:1, Name:"Reds", Colors:["Crimson","Red","Ruby","Maroon"]}，
输出精确字节为
`{"ID":1,"Name":"Reds","Colors":["Crimson","Red","Ruby","Maroon"]}`。
该示例直接调用与端点一致的 encoding/json.Marshal；本轮重新执行既有测试，
由 Go runner 核对真实 stdout 与 Output。两个后端都比较 Rust 保存的物理 bytes，
再将上述 Go bytes 独立写入后读取，预期不由 Rust 编码器生成。

复用并重新执行既有 TestGCStateJSONUtil，其真实 etcd 写入断言包括 `1`、
`"str"`、`null`；本任务两个后端均再次比较这些标量字节。已审查 TestMain、
WaitForEtcdConnections、etcd 测试生命周期及选定测试，无源码写入或 failpoint
注入需求。既有 fixture 仅 build 已有 Go 源码并在槽位内输出二进制和临时数据。

| 只读 Go 命令 | 退出码、有效结果 |
| --- | --- |
| `GOFLAGS=-mod=readonly go test encoding/json -run '^ExampleMarshal$' -count=1 -v` | 0；1 Example passed |
| `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage/endpoint -run '^TestGCStateJSONUtil$' -count=1 -v` | 0；1 顶层测试 passed |

未执行计划列出的完整 `go test -mod=readonly ./pkg/storage/endpoint ./pkg/storage -count=1`：
整包包含需要 failpoint 注入的 TestLoadRegionsExceedRangeLimit 等场景，与本轮 Go
只读约束冲突。仅复用/重跑上述不依赖注入的既有证据，不声称 Go 整包通过。

### 实现和 TDD

canonical config_endpoint.rs 增加 load_config/save_config 及
load_service_middleware_config/save_service_middleware_config，复用 load/save_json
和 EndpointError，错误保持 Store/JsonMarshal/JsonUnmarshal 分类。独立
config_endpoint_test.rs 在 lib.rs 显式注册，无依赖、feature、manifest、lock 或
runtime 变更。

config_endpoint_memory / config_endpoint_etcd 共享同一 contract：缺失、空字节、
Go 对象字节双向读取、覆盖、编码失败保留旧值、坏 JSON/错误形状/非法 UTF-8、
缺省字段、非空 null 的 found 状态、nullable 类型、标量字节及跨 cluster/配置种类隔离。
config_endpoint_go_bytes 为单独纯值编码断言。真实 etcd 额外验证两个配置在
stop/stopped → start/restarted 同 WAL 重启后的恢复；随后 drop fixture 等待子进程
退出，再进行两个端点各自的读/写，四项均为真实 Store 错误，无模拟事务结果。

最小可编译的 load 接口先返回 None，同一聚焦命令
`cargo test --locked -p astersql-pd-storage-endpoint --lib config_endpoint -- --nocapture`
退出 **101**，1 passed / 2 failed / 0 ignored，4.55s；两个后端在实际保存成功后
读取为 None、预期 Some(Config{ID:1,...}) 的状态断言失败。实现后第二次运行发现
测试 schema 的 derive 将 [] 当默认 struct 接受，退出 101，1 passed / 2 failed；
保留原断言，修正该测试 schema 的对象解码，不削弱错误预期。最终相同命令退出
**0**，3 passed / 0 failed / 0 ignored，4.43s。每次 Rust 修改后先运行
`cargo fmt --all`，均退出 0。

### 槽位与最终验证

所有 Cargo 阶段均从仓库根依次用原子 mkdir 动态领取首个空闲槽位，实际均为
**1**，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-endpoint、task=9、batch=28、session=config-endpoint 和
持有 shell PID；EXIT/INT/TERM trap 清理自己的 owner/锁，所有 Cargo 与 fixture
子进程同步结束后才释放。保留编译缓存，子进程继承绝对 target 和
GOFLAGS=-mod=readonly。复用 proto 缓存，未触发会解压 Go 文件的 proto build.rs；
未运行 failpoint 启停、Go 生成器、格式化或依赖整理。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint target 保持注册 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 个 target 汇总含 doc-tests，521 passed / 0 failed / 1 既有 ignored；endpoint 22 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

最终复核开始时 1101 个已跟踪 Go/模块文件 SHA-256 均未变化；Go Git 差异为空，
未跟踪 Go/模块文件为 0。Cargo manifest/lock 无变化，workspace fmt 未修改其他
Rust 文件。审查范围为 config_endpoint.rs、独立测试、lib 注册、本证据节及编号
任务文件清理；用户已有其他计划变更保留。共享槽位锁已释放，缓存保留。
批次 28 的配置持久化验证故事已完成。

## 批次 29：调度器原始配置端点（任务 10，2026-10-23）

### 准入和 Go 依据

批次 28 提交 `0dc4f5a30` 的 config_endpoint.rs、独立测试和本文证据齐全：
聚焦 3 passed，workspace 521 passed / 0 failed / 1 既有 ignored。
已读 ADR-001、porting-guide、执行技能、Copilot 与 canonical endpoint crate；
`rg --files --hidden -g AGENTS.md pkg docs .plans` 未发现更深指令。
config.go 相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无变化。
计划总纲和来源主题只读，本任务不迁移调度器类型、算法或配置校验。

Go `LoadSchedulerConfig` / `SaveSchedulerConfig` / `RemoveSchedulerConfig`
委托 Base 单键读写删；Save 仅将 []byte 转为 string，不进行 JSON 编解码。
缺失与空配置都读取为空；重复删除成功；覆盖无 lease。`LoadAllSchedulerConfigs`
在 SchedulerConfigPathPrefix（尾斜线）到 GetPrefixRangeEnd 之间仅执行一次
LoadRange，limit 是 meta.go 的 MinKVRangeLimit=100，不做分页。
Go memoryKV 的 BTree 和 etcd 的默认范围读均按原始键字节升序，TrimPrefix
只移除前缀，不清理 scheduler 名称里的斜线/父目录。以上是源码依据，
不声称获得了新增 scheduler Go 测试的运行差分。

只读重跑的相邻现有测试：

| 命令 | 退出码、有效结果 |
| --- | --- |
| `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage -run '^TestBasic$' -count=1 -v` | 0；1 顶层测试 passed |
| `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage/endpoint -run '^TestGCStateJSONUtil$' -count=1 -v` | 0；1 顶层测试 passed，真实 etcd |

复核上述 TestMain、WaitForEtcdConnections、选定测试及既有 fixture 调用链，
无源码写入、生成器或 failpoint 注入需求。完整计划 Go 命令
`go test -mod=readonly ./pkg/storage/endpoint ./pkg/storage -count=1` 未执行：
其中 TestLoadRegionsExceedRangeLimit 等需要 failpoint 注入，与 Go 只读约束冲突。
本任务没有编码器，输入原始 bytes 就是应存储的 bytes，不要求新的 JSON/protobuf
输出。测试直接比对物理存储字节，含无效 JSON、非法 UTF-8、NUL、空串；不把
Rust 生成结果当作 Go 编码预期。已有 keypath 的 Go oracle 输出涵盖空名、
nested/name、/a//../b/ 和 Unicode 名称，本任务复用其 canonical 路径函数。

### 实现和测试驱动证据

scheduler_endpoint.rs 在 StorageEndpoint 上新增 load_scheduler_config、
save_scheduler_config、remove_scheduler_config、load_all_scheduler_configs。
单键操作复用 Base，列举使用 canonical MetaStore 线性一致前缀读、limit=100，
返回 `(name_bytes, config_bytes)` 配对，避免 Go string 转 UTF-8 引入数据修复。
名称输入沿用 ClusterPaths 的 &str 契约；列举也保留外部直接写入的非法 UTF-8 名称。
后端错误保持 EndpointError::Store；无新增 runtime、依赖、feature 或锁文件变化。
独立 scheduler_endpoint_test.rs 由 lib.rs 显式注册。

scheduler_endpoint_memory / scheduler_endpoint_etcd 共享同一断言：缺失、空配置、
空名称、原始字节、覆盖、删除/重复删除、名称原样保留、跨 cluster 和相邻前缀
隔离、去前缀与稳定排序，以及 101 项只列出前 100 项（第 101 项仍可单独读取）。
额外直接写入非法 UTF-8 后缀的物理键，验证列举不丢失字节。
etcd 入口验证非空、空配置和已删除项在 stop/stopped → start/restarted 同 WAL
重启后的终态；随后 drop fixture 等待服务器实际退出，再逐一调用四个操作，
全部返回真实 Store 错误，没有模拟核心存储或事务结果。

TDD 最小可编译接口先将列举实现为返回空列表，保存/读取/删除已连接真实后端。
`cargo fmt --all` 退出 0 后运行
`cargo test --locked -p astersql-pd-storage-endpoint --lib scheduler_endpoint -- --nocapture`
退出 **101**：0 passed / 2 failed / 0 ignored（4.22s）。两个入口都在实际保存成功后
列举为 []、预期五组有序名称/原始字节的状态断言失败。补齐范围读后，相同命令
退出 **0**：2 passed / 0 failed / 0 ignored（5.12s）。追加名称边界断言后先重新
`cargo fmt --all`，同一聚焦命令退出 **0**：2 passed / 0 failed / 0 ignored（5.64s）。
原红灯断言保留，未忽略测试或削弱验收。

### 槽位和仓库基线

三个 Cargo 阶段均从仓库根按 1–10 顺序用原子 mkdir 动态领取第一个空闲槽位，
实际均为 **1**，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁 owner 记录 plan=storage-endpoint、task=10、batch=29、session=scheduler-endpoint
和持有 shell PID（68385、68591、68731）；EXIT/INT/TERM trap 清理自己的 owner/锁，
同步等待 Cargo 和 fixture 子进程后释放，缓存保留。所有子进程继承绝对 target 和
GOFLAGS=-mod=readonly。复用 proto 构建缓存，未触发会解压 Go 文件的 build.rs。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint target 保持注册 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 个 target 汇总含 doc-tests，523 passed / 0 failed / 1 既有 ignored；endpoint 24 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

最终核对开始时 1101 个已跟踪 Go/模块文件 SHA-256 全部未变化，Go Git 差异和
未跟踪 Go/模块文件均为空。未运行 Go 生成器、格式化、依赖整理或 failpoint
启停；Cargo manifest/lock 无变化，workspace fmt 未修改其他 Rust 文件。
审查范围仅 scheduler_endpoint.rs、scheduler_endpoint_test.rs、lib.rs 注册、
本证据节与编号任务清理；用户已有其他计划变更保留。共享锁释放，缓存保留。
批次 29 的调度器原始配置持久化验证故事已完成。

## 批次 30：规则文档端点（任务 11，2026-10-23）

### 准入、Go 依据和字节证据

批次 29 提交 `a397c0b12`、scheduler_endpoint 产物和本文完成证据齐全：
聚焦 2 passed，workspace 523 passed / 0 failed / 1 既有 ignored，满足准入。
已读执行技能、ADR-001、porting-guide、Copilot 和 canonical endpoint/kv；
`rg --files --hidden -g AGENTS.md pkg docs .plans` 无更深指令。
rule.go 对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。
plan.md 和来源主题计划保持只读。

Go rule.go 的三类 Save 调用 util.go::saveJSONInTxn：先 json.Marshal，
失败返回 JSONMarshal 错误且不缓存该 put；成功交给调用者事务。Delete 缓存单键删除，
RunInTxn 复用 Base 回调事务，不复制事务引擎。LoadRule 单键读取原始 string，
缺失/空值均为空。三种 Load* 复用 loadRangeByPrefix，每页 MinKVRangeLimit=100，
每页按键字节升序回调，去掉前缀并保持原始值，不解码 JSON；满页后 cursor=最后键+NUL，
后续读取失败不撤销已经交付的回调，不固定 snapshot revision。
三类前缀分别为 /pd/C/rules/、rule_group/、region_label/，沿用 ClusterPaths。
这些为源码依据，没有将它们称为新增 Go rule 测试的运行差分。

复用已保存 Go encoding/json ExampleMarshal 实际输出：输入
ColorGroup{ID:1, Name:"Reds", Colors:["Crimson","Red","Ruby","Maroon"]}，输出
`{"ID":1,"Name":"Reds","Colors":["Crimson","Red","Ruby","Maroon"]}`。
重新执行已有 Go runner 核对 stdout；Rust 三类存储分别比较此对象字节、`1`、`null`，
后两者复用已有 TestGCStateJSONUtil 真实 etcd 断言。分页时将 Go 对象原始字节直接写入
真实后端再读取，没有以 Rust 编码结果构造预期；另有独立纯值 JSON 字节测试。
泛型 schema adapter 负责 Go 字段/nullable/自定义编码，不能推断任意 Serde 类型与
Go JSON 天然同构。本任务不实现 placement 算法或完整规则业务模型。

| 只读 Go 命令 | 退出码与有效结果 |
| --- | --- |
| `GOFLAGS=-mod=readonly go test encoding/json -run '^ExampleMarshal$' -count=1 -v` | 0；1 Example passed |
| `GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/storage/endpoint -run '^TestGCStateJSONUtil$' -count=1 -v` | 0；1 顶层测试 passed，真实 etcd |

已审查选定测试、TestMain/WaitForEtcdConnections、newEtcdStorageEndpoint 与既有 fixture
调用链，无 Go 写入或 failpoint 注入需求。未执行完整计划 Go oracle 命令
`go test -mod=readonly ./pkg/storage/endpoint ./pkg/storage -count=1`，因为整包的
TestLoadRegionsExceedRangeLimit 等需要 failpoint 注入，与 Go 只读约束冲突；
没有声称 Go 整包通过，也没有新增 Go 工具。

### 实现、行为红灯与绿灯

新增 canonical rule_endpoint.rs：StorageEndpoint::{run_in_txn,load_rule,
save_rule,delete_rule,save_rule_group,delete_rule_group,save_region_rule,
delete_region_rule,load_rules,load_rule_groups,load_region_rules}。
事务入口转发现有 Base::run_in_txn 和 CallbackTxn；不增加 runtime、全局状态、依赖、
feature 或第二套 MetaStore。三个列表共享一个真实存储分页实现，以字节 slice 回调，
保留非法 UTF-8、NUL、未清理的名称以及非法 JSON 值。
独立 rule_endpoint_test.rs 在 lib.rs 显式注册。

两个后端共享 contract：缺失/空列表、编码字节、提交前不可见、三类前缀隔离、
编码失败撤销先前缓存写删、真实并发修改导致 compare 冲突且无部分提交、删除、
不同事务内重复删除、同一批次重复键被现有事务接口拒绝且状态不变、相邻前缀和
跨 cluster 隔离，以及每类 205 项的跨页字节顺序和原始字节保留。
etcd 额外验证三类原子写入后同 WAL stop/stopped → start/restarted 恢复。
随后在规则列表第 100 次回调内 drop fixture 并等待服务器退出：下一页返回真实
Store 错误，最终恰好交付 100 项；断连后另行验证三类列表、单键 load、事务提交错误。
没有模拟核心事务结果。

最小可编译列表先返回空，save/load/delete 使用真实后端。
`cargo fmt --all` 退出 0 后，执行
`cargo test --locked -p astersql-pd-storage-endpoint --lib rule_endpoint -- --nocapture`：
退出 **101**，1 passed / 2 failed / 0 ignored（4.63s）；两个真实后端都在成功保存后
列举 []、预期 Go 对象字节一项的状态断言失败。实现分页后发现测试在同一事务内重复
删除同键，被两后端一致拒绝（InvalidArgument）；保留该错误及回滚断言，用两个独立
事务验证删除幂等性。修正后同命令退出 **0**，3 passed / 0 failed / 0 ignored（7.69s）。
原红灯断言保留，没有忽略测试或削弱断言。增加第 100 项关闭真实 etcd 的断连断言后，
先运行 `cargo fmt --all` 再运行相同聚焦命令；最终结果见下表。

### 共享槽位和最终基线

所有 Cargo 阶段均从仓库根按 1–10 顺序以原子 mkdir 动态领取首个空槽，实际为 **1**；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-endpoint、task=11、batch=30、session=rule-endpoint 和
shell PID（71622、71838、72005、72160，以各阶段日志为准）；EXIT/INT/TERM trap
覆盖退出清理，Cargo/fixture 子进程同步结束后才释放 owner/锁，保留缓存。
所有子进程继承绝对 target 和 GOFLAGS=-mod=readonly。复用 proto 构建缓存，
没有触发会解压 Go 文件的 build.rs；fixture 仅 build 已有只读 Go 工具。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib rule_endpoint -- --nocapture` | 0；3 passed / 0 failed / 0 ignored，8.26s |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint target 保持注册 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 个 target 汇总含 doc-tests，526 passed / 0 failed / 1 既有 ignored；endpoint 27 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

最终核对任务开始记录的 1101 个已跟踪 Go/模块文件 SHA-256 均未变化，
Go Git 差异为空，未跟踪 Go/模块文件为 0。Cargo manifest/lock 无变化，
workspace fmt 未改动其他 Rust 文件。已审查仅本任务 rule_endpoint 实现、独立测试、
lib 注册、本证据节和编号任务清理；用户已有其他计划变更保留。
所有 Cargo/fixture 子进程已结束，共享槽位锁已释放，缓存保留。
批次 30 当前规则存储验证故事已完成。

## 批次 31：亲和组端点（任务 12，2026-10-23）

### 准入和只读 Go 证据

批次 30 提交 `ca5e49594`、rule_endpoint 产物和本文完成记录齐全：
聚焦 3 passed，workspace 526 passed / 0 failed / 1 既有 ignored，满足准入。
已读 do-task-plans、ADR-001、porting-guide、Copilot 和 canonical endpoint/kv；
`rg --files --hidden -g AGENTS.md pkg docs .plans` 未发现更深指令。
`affinity.go` 相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无变化。
总计划和来源主题保持只读。

Go `affinity.go` 的 SaveAffinityGroup 经 `util.go::saveJSONInTxn` 先 JSON 编码、
成功才缓存 put；DeleteAffinityGroup 仅缓存精确 group ID 删除，复用调用者事务。
LoadAffinityGroup 缺失/空值返回空 string，读取不解码 JSON。
LoadAllAffinityGroups 复用 loadRangeByPrefix，100 项分页、去前缀、字节排序，
满页从最后 key+NUL 续读；后页失败保留已经调用的回调，不固定快照 revision。
`pkg/schedule/affinity/manager_test.go::TestBasicGroupOperations` 已有创建 group1、
更新 peers、删除并断言不存在的源码证据；本任务不迁移 affinity 算法。
这些是静态源码和既有断言依据，不冒充本轮 Go affinity 运行差分。

编码复用 endpoint_codec_test.rs / rule_endpoint_test.rs 和批次 21/30 已保存的
真实 Go encoding/json ExampleMarshal 输出，输入 ColorGroup{ID:1, Name:"Reds",
Colors:["Crimson","Red","Ruby","Maroon"]}，输出
`{"ID":1,"Name":"Reds","Colors":["Crimson","Red","Ruby","Maroon"]}`。
Rust 独立纯值测试和两个真实后端的保存/读取均与该固定字节比较；标量 `1` 也复用
批次 30 的既有 Go JSON 证据。没有从 Rust 结果生成预期。
ClusterPaths::affinity_group_path / affinity_groups_prefix 复用已有 Go keypath 输出。
泛型持久化模型仍须提供 Go 兼容字段、nullable 和自定义序列化，未宣称任意 Serde
模型与 Go encoding/json 自动同构。

本轮没有运行 Go 测试；完整命令
`go test -mod=readonly ./pkg/storage/endpoint ./pkg/storage -count=1` 涉及
TestLoadRegionsExceedRangeLimit 等 failpoint 测试，与只读约束冲突。必要字节证据
已由上述保存输出提供。审查既有 etcd fixture 调用链：只读构建现有 main.go，
随机 loopback 端口，独立数据/WAL 目录，状态文件均位于共享 Cargo 槽位。

### 实现和测试驱动证据

新增 affinity_endpoint.rs，在 StorageEndpoint 提供 load_affinity_group、
save_affinity_group、delete_affinity_group、load_all_affinity_groups；
事务复用已有 run_in_txn / CallbackTxn。仅将 rule_endpoint.rs 既有分页 helper
可见性调整为 pub(super)，供当前模块复用，不复制分页器或事务引擎。
lib.rs 显式注册独立 affinity_endpoint_test.rs；无新增依赖、feature 或 runtime。

affinity_endpoint_memory / affinity_endpoint_etcd 共用同一 contract，覆盖：缺失、空值、
提交前不可见、保存 Go 编码字节、相邻 group1/group10 精确删除、重复删除、
编码错误撤销此前缓存的写删、真实并发修改触发 compare conflict 且无部分提交、
跨 cluster / 相邻前缀 / rules 前缀隔离，以及 205 项跨三页列举。
分页包含空 ID、g098+NUL、未清理路径和非法 UTF-8 后缀；无效 JSON、非法 UTF-8、
NUL 和空值保持原始字节，单键坏 JSON 也原样返回。
etcd 入口另行验证同 WAL stop/stopped → start/restarted 后对象字节恢复。
随后在列举第 100 项回调内 drop fixture 并等待真实 etcd 退出，第二页返回 Store 错误，
终态恰好交付 100 项；断连后单键读取和事务提交也返回真实 Store 错误。
没有模拟核心事务结果。

最小可编译接口先将列举实现为空，保存/读取/删除连接真实后端。
运行 `cargo fmt --all`（退出 0）后执行
`cargo test --locked -p astersql-pd-storage-endpoint --lib affinity_endpoint -- --nocapture`：
退出 **101**，1 passed / 2 failed / 0 ignored，3.93s；两个后端都在成功提交后
列举 [] 而非 group1/group10 的真实状态断言失败。
接入既有分页器、再次 `cargo fmt --all`（退出 0）后，同一命令退出 **0**：
3 passed / 0 failed / 0 ignored，5.20s；原红灯断言保留，未忽略测试或削弱断言。

### 共享槽位和仓库基线

三个 Cargo 阶段从仓库根按 1–10 顺序用原子 mkdir 动态领取首个空槽，实际均为 **1**。
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-endpoint、task=12、batch=31、session=affinity-endpoint 及
shell PID（75047、75202、75352）。EXIT/INT/TERM trap 清理自己的记录和锁，
Cargo/fixture 子进程同步结束后释放，保留编译缓存。
所有子进程继承绝对 target 和 GOFLAGS=-mod=readonly；复用 proto 构建缓存，
未触发会解压 Go 文件的 build.rs。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib affinity_endpoint -- --nocapture` | 0；3 passed / 0 failed / 0 ignored，5.20s |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint target 保持注册 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 个 target 含 doc-tests，529 passed / 0 failed / 1 既有 ignored；endpoint 30 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

最终核对任务开始快照中的 1101 个已跟踪 Go/模块文件 SHA-256 全部未变化，
Go Git 差异为空，未跟踪 Go/模块文件为 0。未运行 Go 生成器、failpoint 启停、
Go 格式化或依赖整理。Cargo manifest/lock 无变化，workspace fmt 未修改其他 Rust 文件。
审查仅本任务 affinity 模块、独立测试、lib 注册、分页 helper 最小可见性连线及本证据节；
用户已有其他计划变更保留。所有 Cargo/fixture 子进程已结束，共享锁已释放，缓存保留。
状态：已完成；批次 31 的亲和组文档存储验证故事完成。

## 批次 32：复制模式状态端点（任务 13，2026-10-23）

### 准入和只读 Go 依据

批次 31 提交 `b4d012165`、affinity_endpoint 产物和本文完成记录齐全：
聚焦 3 passed，workspace 529 passed / 0 failed / 1 既有 ignored，满足准入。
已读 do-task-plans、ADR-001、porting-guide、Copilot 和 canonical endpoint/kv；
`rg --files --hidden -g AGENTS.md pkg docs .plans` 无更深指令。
`replication_status.go` 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。总计划及来源主题只读。

Go `LoadReplicationStatus` 精确读取 `ReplicationModePath(mode)`；缺失或空 string
返回 false，存储错误直接传播；非空值 json.Unmarshal，坏 JSON 包装为
ErrJSONUnmarshal，成功返回 true。`SaveReplicationStatus` 经 util.go::saveJSON /
saveJSONInTxn 先 json.Marshal，编码失败不写，成功覆盖单键且不附 lease。
keypath 的格式为 `/pd/{cluster_id}/replication_mode/{mode}`，不清理 mode。
Rust 返回新模型的 Option 表达 found，不修改调用者已有结构；模型负责 Go 字段映射、
nullable 和自定义编码，与既有 config adapter 契约一致，不实现复制模式状态机。

字节证据复用批次 21/30 保存且经 Go runner 核验的 encoding/json ExampleMarshal：
输入 ColorGroup{ID:1, Name:"Reds", Colors:["Crimson","Red","Ruby","Maroon"]}，
输出 `{"ID":1,"Name":"Reds","Colors":["Crimson","Red","Ruby","Maroon"]}`。
独立纯值测试和两后端保存/读取均与这些固定字节比较；`1`、`null` 复用已保存的
TestGCStateJSONUtil 输出，null 默认值依据既有 Go TestUnmarshalNulls 证据。
mode 路径复用 member_config_paths_test.rs 已保存 Go 输出。
没有用 Rust 编码结果生成预期，没有将源码依据称作本轮 Go 运行差分。

本轮没有执行完整 Go oracle
`go test -mod=readonly ./pkg/storage/endpoint ./pkg/storage -count=1`：整包包含需要
failpoint 注入的测试，与只读约束冲突；必要编码证据由上述保存输出提供。
审查复用 etcd fixture：只读 build 现有 main.go、GOFLAGS=-mod=readonly，
随机 loopback 端口，独立数据目录，同 WAL 重启，无 Go 写入或 failpoint 启停。
proto build.rs 会解压含 Go 文件的归档；本轮复用已有编译缓存，未触发该脚本。

### 实现和测试驱动证据

新增 canonical replication_endpoint.rs，提供
StorageEndpoint::{load_replication_status,save_replication_status}，复用 ClusterPaths、
load、save_json 和既有 EndpointError。lib.rs 显式注册独立 replication_endpoint_test.rs。
无新增依赖、feature、runtime、全局状态或第二套 MetaStore。

replication_endpoint_memory / replication_endpoint_etcd 共用 contract：缺失、空值、
JSON 往返及 Go 字节、编码错误保留旧值、无效 JSON/错误字段类型/尾随数据、
非空 null 的 found/default 语义、标量，以及多 mode、跨 cluster 隔离。
mode 包含空名称、嵌套路径、未清理路径、中文和 NUL；覆盖一个 mode 后另一模式保持原值。
etcd 入口在保存结束后 stop/stopped → start/restarted，同 WAL 恢复后两个 mode
分别为原对象与 7。随后 drop fixture 并等待服务器退出，再执行读取和保存，
均返回真实 Store 错误；不模拟核心存储或事务结果。

最小可编译接口先真实保存并读取后返回 None。
`cargo fmt --all` 退出 0 后执行
`cargo test --locked -p astersql-pd-storage-endpoint --lib replication_endpoint -- --nocapture`：
退出 **101**，1 passed / 2 failed / 0 ignored（4.03s），两个后端在保存成功后
读取 None 而预期 Some(ColorGroup) 的真实状态断言失败。
实现解码后重新 `cargo fmt --all`（退出 0），相同命令退出 **0**：
3 passed / 0 failed / 0 ignored（5.30s）。原红灯断言保留，未忽略或削弱测试。

### 共享槽位和仓库基线

三个 Cargo 阶段从仓库根按 1–10 顺序通过原子 mkdir 领取首个空槽，实际均为 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-endpoint、task=13、batch=32、session=replication-endpoint
和持有 shell PID（78280、78435、78580）。EXIT/INT/TERM trap 清理自己的 owner/锁，
同步等待 Cargo 和 fixture 子进程结束才释放，缓存保留；子进程继承绝对 target 和
GOFLAGS=-mod=readonly。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib replication_endpoint -- --nocapture` | 0；3 passed / 0 failed / 0 ignored，5.30s |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint target 保持注册 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 个 target 含 doc-tests，532 passed / 0 failed / 1 既有 ignored；endpoint 33 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

最终核对开始快照中 1101 个 Go/模块文件 SHA-256 全部未变化，Go Git 差异为空，
未跟踪 Go/模块文件为 0。未运行生成器、failpoint 启停、Go 格式化或依赖整理。
Cargo manifest/lock 无变化，workspace fmt 未修改其他 Rust 文件。
审查范围仅 replication_endpoint.rs、独立测试、lib.rs 注册及本证据节；
用户已有其他计划变更保留。所有 Cargo/fixture 子进程结束，共享锁已释放，缓存保留。
状态：已完成；批次 32 的按 mode 隔离复制状态 JSON 验证故事完成。

## 批次 33：时间戳数值端点（任务 14，2026-10-23）

### 准入和只读 Go 依据

批次 32 提交 `f0adad162`、replication_endpoint 产物及本文证据齐全：聚焦
3 passed，workspace 532 passed / 0 failed / 1 既有 ignored，满足准入。
已读 do-task-plans、ADR-001、porting-guide、Copilot 和 canonical endpoint/kv。
`rg --files --hidden -g AGENTS.md pkg docs .plans` 无更深指令。两个 Go 源文件
相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。
总计划和来源主题只读。

min_resolved_ts.go / external_timestamp.go 均使用 strconv.FormatUint(value,16)
保存无前导零的小写十六进制，使用 ParseUint(value,16,64) 读取；缺失/空值为 0，
非法或溢出返回 ErrStrconvParseUint，后端错误传播。接受大写和前导零，拒绝符号、
0x 前缀、下划线和空白；HTTP 包装 struct 的 JSON 字段不属于持久化格式。
没有因任务上下文的 JSON 描述改用 JSON 编码。

复用 Go 1.26.8 已有标准库测试/示例，不新增或改写 Go oracle：

| 只读命令 | 退出码与有效结果 |
| --- | --- |
| `GOFLAGS=-mod=readonly go test strconv -run '^ExampleFormatUint$' -count=1 -v` | 0；1 Example passed，输入 42 的实际 stdout 含 `string, 42`、`string, 2a` |
| `GOFLAGS=-mod=readonly go test internal/strconv -run '^(TestItoa\|TestUitoa\|TestParseUint64Base)$' -count=1 -v` | 0；3 tests passed；TestItoa 实际比较 16→`10`、MAX_I64→`7fffffffffffffff`，TestUitoa 覆盖 u64 上界 |
| `GOFLAGS=-mod=readonly go test fmt -run '^TestSprintf$' -count=1 -v` | 0；1 test passed；既有 uint64(MAX) 的 `%#v` 字节为 `0xffffffffffffffff` |

Rust 保存与纯值测试比较上述固定字节；fmt 的最大值证据仅移除显式 `0x` 前缀，
不称作 FormatUint(MAX,16) 的新增运行输出。2^53+1 和 2^63 等额外值的预期为
源码推导的边界断言，和上述实际 Go 字节证据明确区分，不由 Rust 输出生成预期。
标准库选定用例无生成器、Go 源写入或 failpoint 需求。完整计划 Go oracle
`go test -mod=readonly ./pkg/storage/endpoint ./pkg/storage -count=1` 未运行，
因为整包包含需注入的 failpoint 测试；没有声称 Go 整包通过。
复用既有 etcd fixture，只读 build 现有 main.go，GOFLAGS=-mod=readonly，
随机 loopback 端口、独立数据目录；proto build.rs 使用既有缓存，未触发解压 Go 文件。

### 实现和测试驱动证据

新增 canonical timestamp_values.rs，提供 StorageEndpoint::{load_min_resolved_ts,
save_min_resolved_ts,load_external_ts,save_external_ts}；复用 ClusterPaths 与 Base，
不新增依赖、runtime、feature 或全局 cluster ID。TimestampValueError 区分 ParseUint
和真实 Store 错误。两个端点各读写精确键且不附 lease，显式校验 ASCII hex 后
按 u64 解码，避免 Rust parser 接受 `+1` 的差异。
lib.rs 显式注册独立 timestamp_values_test.rs。

timestamp_values_memory / timestamp_values_etcd 共用真实后端 contract：缺失、空值、
零、16、42、MAX_I64、2^53+1、2^63、MAX_U64 的精确字节和往返；大写、前导零、
十六进制 `10` 的含义；非法 JSON/符号/前缀/空白/下划线/溢出/非 UTF-8/NUL，
读失败保留原始字节；两个端点和 cluster 之间隔离。etcd 成功保存后执行
stop/stopped → start/restarted，同 WAL 恢复后分别读到 42 与 MAX_U64；随后
Drop fixture 并等待进程退出，再执行两个 load/save，四个请求均返回真实 Store 错误。
没有模拟核心存储或事务结果。

最小可编译接口先真实保存/读取、解码返回 0。`cargo fmt --all` 退出 0 后运行
`cargo test --locked -p astersql-pd-storage-endpoint --lib timestamp_values -- --nocapture`：
退出 **101**，1 passed / 2 failed / 0 ignored（4.53s），两个后端都在保存 16 后
读到 0 而预期 16 的真实状态断言失败。实现解码后再次 `cargo fmt --all`，同一命令
退出 **0**，3 passed / 0 failed / 0 ignored（4.60s）。原红灯断言保留，未忽略测试。

### 共享槽位和最终基线

各 Cargo 阶段从根目录按 1–10 原子 mkdir 动态领取，实际槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-endpoint、task=14、batch=33、session=timestamp-values 与
shell PID（81809、81967、82132、83582）；EXIT/INT/TERM trap 覆盖清理，
同步等待 Cargo/fixture 子进程结束后释放自己的 owner/锁，保留缓存。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib timestamp_values -- --nocapture` | 0；3 passed / 0 failed / 0 ignored，6.43s |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint target 保持注册 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 个 target 含 doc-tests，535 passed / 0 failed / 1 既有 ignored；endpoint 36 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

最终核对开始快照中 1101 个 Go/模块文件 SHA-256 全部未变化，Go Git 差异为空，
未跟踪 Go/模块文件为 0。未运行生成器、failpoint 启停、Go 格式化或依赖整理。
Cargo manifest/lock 无变化，workspace fmt 未修改其他 Rust 文件。基线后仅补充
测试注释的 Go 字节来源，并重跑 fmt、fmt check 和全部 3 个聚焦测试成功。
审查范围仅 timestamp_values 实现、独立测试、lib.rs 注册及本证据节；
用户已有其他计划变更保留。所有 Cargo/fixture 子进程结束，共享锁已释放，缓存保留。
状态：已完成；批次 33 的时间戳数值持久化验证故事完成。

## 任务 15：时间窗口读取与删除（批次 34，2026-10-23）

### 准入及只读 Go 依据

核验前序提交 `7886c592a` 与任务 14 证据：聚焦 3 passed、workspace
535 passed / 0 failed / 1 既有 ignored，metadata/fmt/check 退出 0，批次 33 满足准入。
已读执行技能、根规则、Copilot 规则、ADR-001、移植指南、只读总计划与本任务；
`rg --files -g AGENTS.md pkg docs .plans` 未发现更深规则。
只读 Go `tso.go`、typeutil 的 time.go/conversion.go 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

源码依据：`LoadTimestamp` 使用 TimestampPath(groupID)，缺失/空字符串返回
Go ZeroTime，非空交给 ParseTimestamp，错误原样返回；ParseTimestamp 严格读取
8 字节大端 uint64，再按 int64 转换为 time.Unix(0,nano)。`DeleteTimestamp`
在 caller context 控制的 RunInTxn 回调中 Remove 精确键，不读旧值、不删邻居。
`TestSaveLoadTimestamp` 和 `TestTimestampTxn` 是现有保存/加载及旧值保护静态断言，
本任务不涉及 SaveTimestamp。完整 Go oracle 包含需注入的 TestSaveTimestampWithTimeout，
没有执行 `go test -mod=readonly ./pkg/storage/endpoint ./pkg/storage -count=1`，
没有注入 failpoint 或将静态断言冒充本轮运行差分。

编码复用本文件任务 3 保存的真实 Go 运行证据：
`GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/utils/typeutil -run
'^(TestBytesToUint64|TestUint64ToBytes)$' -count=1 -v`，当时退出 0、2 passed，
1000 的实际输出为 hex `00000000000003e8`。本轮两个后端直接写入这些 Go 字节，
读取预期 Unix epoch 后 1000 纳秒；纯值测试另比对已有 Rust u64_to_bytes 输出。
其余正负 i64 边界来自 Go 源码推导，没有声称额外 Go 编码运行输出。

### 实现与测试驱动证据

canonical timestamp_window.rs 扩展 StorageEndpoint::{load_timestamp,delete_timestamp}，
读取返回 Option<SystemTime>：None 显式映射 Go 零时间（year 1），Some(UNIX_EPOCH)
表示实际编码的八字节零，二者不混同。复用 canonical typeutil::parse_timestamp、
ClusterPaths 和 Base/CallbackTxn；唯一依赖变化为 endpoint 引用已存在的 workspace
astersql-pd-typeutil，Cargo.lock 仅增加该依赖边，没有下载新依赖或创建平行 codec。
TimestampWindowError 保留 Decode/Store/Conflict 分类；删除使用 caller CancellationToken，
预取消返回 Cancelled，已开始操作取消返回 Unavailable，不宣称取消证明回滚。
未迁移受保护保存、选主或业务算法。

lib.rs 显式注册独立 timestamp_window_test.rs；timestamp_window_memory /
timestamp_window_etcd 共用同一真实存储 contract：组 0、1、10、100000、u32::MAX，
缺失、空字节、Go 1000 字节、编码 0/1/i64::MAX/i64::MIN/-1、坏长度 1/2/7/9/16，
坏值读取不修改原字节；默认/非默认精确路径、跨 cluster/相邻 group/子路径隔离，
预取消删除保留原值、成功删除、重复删除缺失键。纯值入口 timestamp_window_go_bytes。
etcd contract 完成后 stop/stopped → start/restarted，同 WAL 恢复后检查组 1 仍缺失、
组 0 仍为 1000ns；随后 Drop fixture 并等待进程退出，再 load/delete 都返回真实
Store 错误。没有模拟核心事务或存储结果。无到期场景，本任务未添加 lease。

最小可编译读取接口取得真实 bytes 后返回 None，先 `cargo fmt --all`（退出 0），
因新增已有 crate 依赖先 `cargo check --workspace`（退出 0）同步锁文件。
`cargo test --locked -p astersql-pd-storage-endpoint --lib timestamp_window -- --nocapture`
实际红灯退出 **101**，1 passed / 2 failed / 0 ignored，4.33s；两个后端均在 Go
1000 字节读回断言返回 None、预期 Some(epoch+1000ns) 时失败。
接入已有解码器后先 fmt，同一命令退出 **0**，3 passed / 0 failed / 0 ignored，4.97s。
原红灯断言保留，未降低断言或忽略测试。

### 共享槽位与最终验证

两个 Cargo 阶段均从根目录依次原子 mkdir 动态领取到槽位 **1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-endpoint/task=15/batch=34/session=timestamp-window，
持有 shell PID 为 85118、85307。EXIT/INT/TERM 清理自己的 owner/锁，
等待同步 Cargo 与 fixture 子进程结束再释放，保留编译缓存。GOFLAGS=-mod=readonly；
已有 fixture 只读 build 既有 main.go，数据/日志/二进制均在槽位中；
proto 使用既有编译缓存，未运行其解压 Go 文件的 build.rs。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib timestamp_window -- --nocapture` | 0；3 passed / 0 failed / 0 ignored，4.97s |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint target 注册不变 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 个 target 含 doc-tests，538 passed / 0 failed / 1 既有 ignored；endpoint 39 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

开始/结束 SHA-256 快照的 1101 个 Go/模块文件完全一致，Go Git 差异为空，
未新增 Go/模块文件。没有运行 failpoint、生成器、Go 格式化或依赖整理。
workspace fmt 未修改其他 Rust 文件。已审查全部实现、测试、模块注册、
manifest、唯一 lock 边及本证据节；用户既有计划修改保留。
所有 Cargo/fixture 子进程已结束，自己的锁已释放，槽位缓存保留。
状态：已完成；批次 34 时间窗口读取与删除验证故事完成。

## 任务 16：时间窗口受保护写入（批次 35，2026-10-23）

### 准入及只读 Go 依据

核验批次 34 提交 `222d3a016`、timestamp_window 产物及本文证据：聚焦
3 passed、workspace 538 passed / 0 failed / 1 既有 ignored，基线均退出 0，满足准入。
已读 do-task-plans、根规则、Copilot、ADR-001、porting-guide 和只读计划；
`rg --files --hidden -g AGENTS.md pkg docs .plans` 未发现更深规则。
`tso.go` 相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

Go SaveTimestamp：未授予（空预期领导值）返回 NotLeader；在 RunInTxn 内读取领导值，
不匹配返回 NotLeader；再读取时间戳，非空必须成功解码，使用
SubRealTimeByWallClock(ts, previousTS) <= 0 拒绝相等/倒退；以 UnixNano 转 uint64
的八字节大端编码保存。etcdTxn 的读取值保护与领导值保护一起原子提交，失败不重试。
既有 TestTimestampTxn 静态断言倒退不改旧值，TestSaveTimestampWithLeaderCheck
静态断言未授予、空/缺失领导值都不写入，重新恢复领导值可继续保存。
完整 Go oracle `go test -mod=readonly ./pkg/storage/endpoint ./pkg/storage -count=1`
未运行：包含 TestSaveTimestampWithTimeout 等需注入 failpoint 的测试，遵守只读约束，
不执行注入，也没有将这些源码断言冒充本轮 Go 运行差分。

编码复用本文任务 3 保存的真实 Go TestUint64ToBytes 运行证据：
`GOFLAGS=-mod=readonly go test -mod=readonly ./pkg/utils/typeutil -run
'^(TestBytesToUint64|TestUint64ToBytes)$' -count=1 -v`，当时退出 0、2 passed，
输入 1000 的实际字节为 `00000000000003e8`。本轮 Rust 两个后端实际保存 time=epoch+1000ns
后逐字节比较该输出；其余正负边界来自源码推导，未声称新增 Go 运行输出。
既有 etcd fixture 的 Go build 不调用生成器、不改 Go 源，GOFLAGS=-mod=readonly，
只读编译现有 main.go；proto 复用缓存，没有运行其解压 Go 文件的 build.rs。

### 实现及测试驱动证据

canonical timestamp_fenced_save.rs 新增 StorageEndpoint::save_timestamp 和
TimestampSaveError。接收后端中立 Compare 列表，宿主负责提供已授予 term 的比较条件，
可附 lease/revision 条件；不依赖 member/election，未创建 runtime 或第二套 codec。
无条件列表或空的预期 Value 拒绝为 NotLeader。复用 Base::run_in_txn_with_conditions
与 CallbackTxn::load，将全部 term 条件、时间戳旧值保护及新值写入同一真实事务。
使用已有 typeutil 的 Go wrapping wall-clock 差值和 u64 大端编码，缺失/空值可首次保存。
解码/存储/非递增分别保留错误类别。泛型 Compare 的提交失败统一为 Conflict
（可能是领导条件失效或旧时间戳冲突，无法从后端失败结果区分），没有伪造 election 错误。
预取消为 Cancelled，操作中取消为 Unavailable，明确保留提交结果未知的语义。

显式注册独立 timestamp_fenced_save_test.rs。memory / etcd 共用 contract：
默认组、非默认组和 u32::MAX；首次保存、Go 精确字节、相等/倒退不修改旧值、递增成功；
空条件/空预期值、领导变更/空值/缺失、附加条件失败；损坏时间戳错误保留原字节；
空值首次保存负时间，再增加到 epoch，重复 epoch 拒绝；预取消无写入。
etcd contract 保存后 stop/stopped → start/restarted，同 WAL 恢复读到 1001ns；
Drop fixture 等待真实进程退出，随后 save 返回 Store 错误。

memory_races / etcd_races 共用真实存储竞争断言。DispatchGate 只暂停事务 IO
派发，转发所有真实读取及提交，不模拟核心事务结果：已有值/缺失/空值三种初态，
两次 save 均读完后同时释放提交，恰好一个成功、另一个 Conflict，终态 2000ns；
另一 save 读完后将领导值改为 term-2 再释放，旧 term Conflict，终态仍 2000ns；
lease-bound 领导键在读完后 revoke，再释放提交，Conflict，终态仍 2000ns；
最后在提交派发前取消，返回 Unavailable，门控请求被丢弃，真实后端终态仍 2000ns。
本任务使用明确 revoke 而非自然到期；没有宣称计时到期证据。所有 future 同步 join，
没有后台遗留任务。测试只新增已有 workspace async-trait 的 dev 依赖，lock 仅多一条依赖边。

最小可编译实现仅真实保存，不做单调保护；先 `cargo fmt --all` 退出 0，执行
`cargo test --locked -p astersql-pd-storage-endpoint --lib timestamp_fenced_save -- --nocapture`
红灯退出 **101**：1 passed / 2 failed / 0 ignored，5.47s；memory 和 etcd 都在相等时间
应返回 NotIncreasing 的真实断言失败。接入事务保护后先 fmt，再
`cargo check --workspace` 退出 0 同步唯一 lock 边；同一聚焦命令绿灯退出 **0**：
5 passed / 0 failed / 0 ignored，5.15s。保留原红灯断言并增加两个真实后端竞争入口。

### 共享槽位及最终验证

各阶段从根目录按 1–10 原子 mkdir 动态领取，实际均为槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录 plan=storage-endpoint/task=16/batch=35/session=timestamp-fenced-save，
持有 shell PID 为 88123（红灯）、88380（实现后绿灯）、88629（最终验证）。
EXIT/INT/TERM 清理自己的 owner/锁；同步 Cargo 和 fixture 全部结束后才释放，缓存保留。

| 最终命令 | 退出码与有效结果 |
| --- | --- |
| `cargo fmt --all` | 0；未修改其他任务 Rust 文件 |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib timestamp_fenced_save -- --nocapture` | 0；5 passed / 0 failed / 0 ignored，5.18s |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical endpoint target 注册不变 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server cpu_usage 弃用警告 |
| `cargo test --workspace --locked` | 0；83 个 target 含 doc-tests，543 passed / 0 failed / 1 既有 ignored；endpoint 44 passed |
| `git diff --check`、`git diff --cached --check` | 0 |

开始/结束的 1101 个 Go/模块文件 SHA-256 快照完全一致，无增删、无内容变化，
Go Git 差异为空。没有运行 failpoint、生成器、Go 格式化或依赖整理。
已审查实现、独立测试、显式注册、dev 依赖和唯一 lock 边及本证据节；
总计划及来源主题未修改，用户已有其他计划变更保留。所有 Cargo/fixture 子进程结束，
自己的占用记录/锁已释放。状态：已完成；批次 35 的受保护保存验证故事完成。
