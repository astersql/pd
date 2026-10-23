# 存储键路径覆盖盘点（任务 1）

日期：2026-10-23（Asia/Shanghai）。状态：已完成文档盘点。
本轮读取 HEAD `d36a252c3157c8aeba5a4ff273f2b82d59f4d808`；规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223`。两者之间 keypath Go diff 为空。
只做源码审计，不改变运行行为、不运行 Go/Cargo、不生成 oracle；下文的行为结论
均为源码或既有测试断言依据，不是本轮跨语言运行差分。

## 准入与边界

全局批次 1，无本系列前序依赖。主题 2 准入证据为已提交的
[核心主题回归](core-theme-regression.md)（提交 `aaebaed6a`）及
[覆盖审计](core-coverage-audit.md)。核对当前 `pkg/core/Cargo.toml` 与
`pkg/core/lib.rs` 的生产模块和独立测试注册，core 基线存在。
历史回归记录：core/storelimit/response 共 200 项有效测试；workspace
446 passed、0 failed、1 ignored；本轮未重跑，不将其写成本轮运行结果。
仓库仅根 `AGENTS.md`，已读 Copilot 指南与 ADR-001。

后续编号：2 集群与元数据；3 配置、成员、选主；4 GC；5 规则、资源组；
6 键空间与时间；7 注册、恢复与维护。以下分配是覆盖责任，不代表后续实现已完成。
`AffinityGroup*` 归 5 的规则族路径，实际 JSON/事务归上层 endpoint；
`MsParam`、`Prefix`、store weights、replication mode 归 3；keyspace allocator 归 2。
后续任务需据此核对其行为范围，不能因窄标题而漏项。
全局 `ClusterID/SetClusterID/ResetClusterID` 仅作为 Go oracle 来源：2 以显式上下文替代，
禁止引入 Rust 全局 cluster ID；测试用 reset 不要求生产等价 setter。
业务 value、事务、watch/lease 的实现归来源主题 3 的 endpoint/kv/election/member/ID
或上层 TSO/启动装配，不在本子计划实现。

## 完整来源与快照

下列为 keypath 全部 12 个 Go 文件（10 个生产文件、2 个测试文件），SHA-256
便于后续复核源码变化；文件路径均已检查存在。

| 来源 | SHA-256 |
| --- | --- |
| `pkg/utils/keypath/absolute_key_path.go` | `84b851ba31ba4f974622c5e4b2331387eb11108b3299fe88a5df67ace26a88d3` |
| `pkg/utils/keypath/affinity.go` | `49973f569f0171e43680bea2a3b7d458b45ab8feb59714eaa2d857667cbaff3b` |
| `pkg/utils/keypath/cluster_id.go` | `e63a257d38836ed66b584cf5ed984f7efc1585a7fb95bb039bfc07d246eac292` |
| `pkg/utils/keypath/config.go` | `6c18b9aabfd6d01b3bb487c93dc7afcf697626394f38344489f6484d85366c09` |
| `pkg/utils/keypath/election.go` | `7735930f48835c307d57f501b58e240b5a375d2cad02cdcb4bb6b54b6c9a47ca` |
| `pkg/utils/keypath/gc_states.go` | `bd9ceba1a4c512c0b2e5ac87de254c4be07b00d505c6fcff25fedfd2a52bd1fa` |
| `pkg/utils/keypath/maintenance.go` | `961f8bd61f113be9d27b6ecde00561b0d8a755e6222c6ed828d6e877323b86ae` |
| `pkg/utils/keypath/member.go` | `1569b2eac91fbe8aa57166f06bff45767b684ae78411826d8e14a66b4c439754` |
| `pkg/utils/keypath/region_test.go` | `40c5942039bbece703b947e4bf4bba2a8350e8e7e3aa0e44cd6ada9d48bae225` |
| `pkg/utils/keypath/resource_group.go` | `6bf06facf89fed595f2d3a8d229af5cbda2f6f59303a86f14ff3e01c77ee920a` |
| `pkg/utils/keypath/resource_group_test.go` | `d5689cd481f5051f1925bbff2129561c4c9ecd59cfff9e72d6d86de3808e5be5` |
| `pkg/utils/keypath/rule.go` | `c2da8c59eed2223abe6ee5039b00d4443a9301094611d956f3fb2896ccb4bbab` |

## 键 schema 与编码规则

符号表中的来源行包含准确参数与返回类型；本节列出全部字符串 schema 常量，
避免仅列函数名遗漏字节格式。`%d` 无补零，`%020d/%08d/%05d` 是最小宽度，
不截断超宽值。字符串参数直接 `%s`，空串、斜杠、`..` 不会自动清理/转义。
GC 的 NullKeyspaceID 为 4294967295，DefaultKeyspaceID 为 0，不能混同；
来源 `pkg/keyspace/constant/constant.go`。相对 resource_group 键没有前导斜杠和 cluster。

| `absolute_key_path.go` 常量（含非导出 schema） | 源码值 |
| --- | --- |
| `GCWorkerServiceSafePointID` | `"gc_worker"` |
| `NativeBRServiceSafePointID` | `"native_br"` |
| `ClusterIDPath` | `"/pd/cluster_id"` |
| `leaderPathFormat` | `"/pd/%d/leader"` |
| `allocIDPathFormat` | `"/pd/%d/alloc_id"` |
| `keyspaceAllocIDPathFormat` | `"/pd/%d/keyspaces/alloc_id"` |
| `configPathFormat` | `"/pd/%d/config"` |
| `schedulerConfigPathFormat` | `"/pd/%d/scheduler_config/%s"` |
| `storeLeaderWeightPathFormat` | `"/pd/%d/schedule/store_weight/%020d/leader"` |
| `storeRegionWeightPathFormat` | `"/pd/%d/schedule/store_weight/%020d/region"` |
| `serviceMiddlewarePathFormat` | `"/pd/%d/service_middleware"` |
| `replicationModePathFormat` | `"/pd/%d/replication_mode/%s"` |
| `recoveringMarkPathFormat` | `"/pd/%d/cluster/markers/snapshot-recovering"` |
| `pitrRestoreModeMarkPathFormat` | `"/pd/%d/cluster/markers/pitr-restore-mode"` |
| `memberBinaryDeployPathFormat` | `"/pd/%d/member/%d/deploy_path"` |
| `memberGitHashPath` | `"/pd/%d/member/%d/git_hash"` |
| `memberBinaryVersionPathFormat` | `"/pd/%d/member/%d/binary_version"` |
| `memberLeaderPriorityPathFormat` | `"/pd/%d/member/%d/leader_priority"` |
| `rulePathFormat` | `"/pd/%d/rules/%s"` |
| `ruleCommonPrefixFormat` | `"/pd/%d/rule"` |
| `ruleGroupPathFormat` | `"/pd/%d/rule_group/%s"` |
| `regionLabelPathFormat` | `"/pd/%d/region_label/%s"` |
| `regionLabelPrefixFormat` | `"/pd/%d/region_label/"` |
| `maintenanceTaskPathFormat` | `"/pd/%d/maintenance/%s"` |
| `gcStateRevisionPathFormat` | `"/pd/%d/gc/gc_state_revision"` |
| `unifiedGCSafePointPathFormat` | `"/pd/%d/gc/safe_point"` |
| `keyspaceLevelGCSafePointPrefixFormat` | `"/pd/%d/keyspaces/gc_safe_point/"` |
| `keyspaceLevelGCSafePointPathFormat` | `"/pd/%d/keyspaces/gc_safe_point/%08d"` |
| `unifiedTxnSafePointPath` | `"/tidb/store/gcworker/saved_safe_point"` |
| `keyspaceLevelTxnSafePointPath` | `"/keyspaces/tidb/%d/tidb/store/gcworker/saved_safe_point"` |
| `unifiedGCBarrierPathFormat` | `"/pd/%d/gc/safe_point/service/%s"` |
| `keyspaceLevelGCBarrierPathFormat` | `"/pd/%d/keyspaces/service_safe_point/%08d/%s"` |
| `globalGCBarrierPathFormat` | `"/pd/%d/gc/global_gc_barriers/%s"` |
| `globalGCBarrierPathPrefix` | `"/pd/%d/gc/global_gc_barriers/"` |
| `unifiedTiDBMinStartTSPrefix` | `"/tidb/server/minstartts/"` |
| `keyspaceLevelTiDBMinStartTSPrefix` | `"/keyspaces/tidb/%d/tidb/server/minstartts/"` |
| `gcSafePointV2PrefixFormat` | `keyspaceLevelGCSafePointPrefixFormat` |
| `gcSafePointV2PathFormat` | `keyspaceLevelGCSafePointPathFormat` |
| `serviceSafePointV2PathFormat` | `keyspaceLevelGCBarrierPathFormat` |
| `clusterPathFormat` | `"/pd/%d/raft"` |
| `clusterBootstrapTimePathFormat` | `"/pd/%d/raft/status/raft_bootstrap_time"` |
| `storePathPrefixFormat` | `"/pd/%d/raft/s/"` |
| `storePathFormat` | `"/pd/%d/raft/s/%020d"` |
| `minResolvedTSPathFormat` | `"/pd/%d/raft/min_resolved_ts"` |
| `externalTimestampPathFormat` | `"/pd/%d/raft/external_timestamp"` |
| `keyspaceMetaPrefixFormat` | `"/pd/%d/keyspaces/meta/"` |
| `keyspaceMetaPathFormat` | `"/pd/%d/keyspaces/meta/%08d"` |
| `keyspaceIDPathFormat` | `"/pd/%d/keyspaces/id/%s"` |
| `keyspaceGroupIDPrefixFormat` | `"/pd/%d/tso/keyspace_groups/membership/"` |
| `keyspaceGroupIDPathFormat` | `"/pd/%d/tso/keyspace_groups/membership/%05d"` |
| `keyspaceGroupIDPattern` | `tso/keyspace_groups/membership/(\d{5})$` |
| `servicePathFormat` | `"/ms/%d/%s/registry/"` |
| `registryPathFormat` | `"/ms/%d/%s/registry/%s"` |
| `msPrimaryPathFormat` | `"/ms/%d/%s/primary"` |
| `msTsoDefaultPrimaryPathFormat` | `"/ms/%d/tso/00000/primary"` |
| `msTsoKespacePrimaryPathFormat` | `"/ms/%d/tso/keyspace_groups/election/%05d/primary"` |
| `msExpectedPrimaryPathFormat` | `"/ms/%d/%s/primary/expected_primary"` |
| `msTsoDefaultExpectedPrimaryPathFormat` | `"/ms/%d/tso/00000/primary/expected_primary"` |
| `msTsoKeyspaceExpectedPrimaryPathFormat` | `"/ms/%d/tso/keyspace_groups/election/%05d/primary/expected_primary"` |
| `keyspaceResourceGroupSettingsPathPrefixFormat` | `"resource_group/keyspace/settings/"` |
| `keyspaceResourceGroupSettingsPathFormat` | `"resource_group/keyspace/settings/%d/%s"` |
| `keyspaceResourceGroupStatesPathPrefixFormat` | `"resource_group/keyspace/states/"` |
| `keyspaceResourceGroupStatesPathFormat` | `"resource_group/keyspace/states/%d/%s"` |
| `keyspaceServiceLimitsPathPrefixFormat` | `"resource_group/keyspace/service_limits/"` |
| `keyspaceServiceLimitsPathFormat` | `"resource_group/keyspace/service_limits/%d"` |
| `resourceGroupSettingsPathFormat` | `"resource_group/settings/%s"` |
| `resourceGroupStatesPathFormat` | `"resource_group/states/%s"` |
| `controllerConfigPath` | `"resource_group/controller"` |
| `timestampPathFormat` | `"/pd/%d/timestamp"` |
| `msTimestampPathFormat` | `"/ms/%d/tso/%05d/gta/timestamp"` |
| `affinityGroupPathFormat` | `"/pd/%d/affinity_groups/%s"` |
| `metaServiceGroupStatusFormat` | `"/pd/%d/meta_service_groups/%s/status"` |
| `keyLen` | `20` |
| `regionPathPrefix` | `"raft/r"` |

反例与边界（源码推导，后续任务应固定 Go 输出再验证 Rust）：

- `Prefix` 使用 `path.Dir`，空串为 `.`，`a/../b` 的目录为 `.`；只有此函数
  清理路径。任务 3 所写“path.Join 清理规则”需以实际源码为准，其他构造器不调用 Join。
- `ExtractStoreIDFromPath` 仅 TrimPrefix 当前集群 store prefix，再 TrimLeft 全部 `0`，
  最后 ParseUint(10,64)。StorePath(0) 反解因空串失败；裸 `123` 也可解析，
  错集群完整路径、非数字、溢出失败，不是严格前缀校验器。
- `ElectionPath(nil)` 或空 ServiceName 用 `/pd/C/leader`；tso group 0 用
  `/ms/C/tso/00000/primary`，其他 group 用 election/%05d；其他服务忽略 GroupID。
  `ExpectedPrimaryPath` 不保护 nil（Go 解引用 panic），空服务名仍直接格式化，
  不可误复用 ElectionPath 的 PD leader 分支。MsParam 字段为 ServiceName/GroupID。
- `TimestampPath(0)` 为 `/pd/C/timestamp`，非零为 `/ms/C/tso/%05d/gta/timestamp`。
  Group regexp 为 `tso/keyspace_groups/membership/(\d{5})$`，无起始锚；
  6 位 group 的构造结果不命中，不应擅自收紧/放宽两者。
- `GCSafePointPath`、`GCBarrierPath`、TxnSafePointPath、CompatibleTiDBMinStartTSPrefix
  仅对 Null 分支；keyspace GC ID 补 8 位，TiDB 兼容 ID 不补零且不带 cluster。
  V2 API 总走 keyspace 格式（含 Null）；ServiceGCSafePoint* 是 Null barrier 别名。
- 所有以空参数获得的 prefix 保留 schema 尾 `/`；`RuleCommonPathPrefix` 特意为
  `/pd/C/rule`，共同命中 rules 与 rule_group；不可补 `/`。
- `ParseKeyspaceResourceGroupPath` 接受去掉 settings/states prefix 后的相对后缀：
  至少两个 slash 分段，首段 ParseUint(10,32)，剩余原样作为组名，允许嵌套与空尾。
  `123/` 得空名，`123//x` 得 `/x`；无 slash、非数字与超 uint32 错误；
  错误文本分别为 `invalid keyspace resource group setting path: ...` 与
  `invalid keyspace ID str: ...`。Null 资源组走 legacy，其余 ID 用不补零十进制。

## 逐个公开符号与反向依赖

下表穷举生产公开函数与类型；调用方列为仓库所有 Go import（含别名）扫描出的
直接符号引用，每个文件保留首次引用行；同包内部组合调用另见真实函数体。
`测试 N 文件` 是静态引用数，不是运行测试数。无外部引用不表示可以省略公开 API。
常量 `ClusterIDPath`、`GCWorkerServiceSafePointID`、`NativeBRServiceSafePointID`
及类型字段见上面的 schema 表与 MsParam 源码；service ID 常量归任务 4。

| 公开声明与来源 | 后续任务 | 生产调用方（文件:首次行）/测试引用 |
| --- | --- | --- |
| `pkg/utils/keypath/absolute_key_path.go:143`<br>`type MsParam struct` | 3 | `pkg/mcs/utils/expected_primary.go:42`；`pkg/mcs/scheduling/server/server.go:492`；`pkg/mcs/resourcemanager/server/server.go:426`；`pkg/tso/allocator.go:289`；`pkg/tso/keyspace_group_manager.go:798`；`pkg/member/participant.go:54`；`server/server.go:2459`；`pkg/keyspace/tso_keyspace_group.go:2261`；测试 3 文件 |
| `pkg/utils/keypath/absolute_key_path.go:149`<br>`func Prefix(str string) string` | 3 | 无包外生产直接引用；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:154`<br>`func AllocIDPath() string` | 2 | `pkg/id/id.go:165`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:159`<br>`func KeyspaceAllocIDPath() string` | 2 | `pkg/id/id.go:163`；`pkg/gc/enabled_keyspace_loader.go:64`；测试 1 文件 |
| `pkg/utils/keypath/absolute_key_path.go:164`<br>`func RegistryPath(serviceName, serviceAddr string) string` | 7 | `pkg/mcs/discovery/register.go:94`；测试 3 文件 |
| `pkg/utils/keypath/absolute_key_path.go:169`<br>`func ServicePath(serviceName string) string` | 7 | `pkg/mcs/discovery/discover.go:33`；`pkg/tso/keyspace_group_manager.go:519`；`pkg/keyspace/tso_keyspace_group.go:581`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:174`<br>`func ClusterPath() string` | 2 | `pkg/storage/endpoint/meta.go:68`；`server/server.go:815`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:179`<br>`func ClusterBootstrapTimePath() string` | 2 | `pkg/storage/storage.go:248`；`server/server.go:821`；`server/cluster/cluster.go:325`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:184`<br>`func StorePath(storeID uint64) string` | 2 | `pkg/storage/endpoint/meta.go:79`；`server/server.go:829`；测试 1 文件 |
| `pkg/utils/keypath/absolute_key_path.go:189`<br>`func StorePathPrefix() string` | 2 | `pkg/mcs/router/server/meta/watcher.go:110`；`pkg/mcs/scheduling/server/meta/watcher.go:144`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:194`<br>`func ExtractStoreIDFromPath(path string) (uint64, error)` | 2 | `pkg/mcs/router/server/meta/watcher.go:93`；`pkg/mcs/scheduling/server/meta/watcher.go:114`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:200`<br>`func MinResolvedTSPath() string` | 6 | `pkg/storage/endpoint/min_resolved_ts.go:40`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:205`<br>`func ExternalTimestampPath() string` | 6 | `pkg/storage/endpoint/external_timestamp.go:40`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:210`<br>`func RecoveringMarkPath() string` | 7 | `server/server.go:2362`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:215`<br>`func PitrRestoreModeMarkPath() string` | 7 | `server/server.go:2392`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:220`<br>`func KeyspaceMetaPrefix() string` | 6 | `pkg/mcs/scheduling/server/keyspace_meta/watcher.go:70`；`pkg/gc/gc_state_lifecycle.go:93`；`pkg/storage/endpoint/keyspace.go:128`；`server/keyspace_service.go:116`；测试 2 文件 |
| `pkg/utils/keypath/absolute_key_path.go:225`<br>`func KeyspaceMetaPath(spaceID uint32) string` | 6 | `pkg/storage/endpoint/keyspace.go:57`；`pkg/keyspace/keyspace.go:362`；测试 1 文件 |
| `pkg/utils/keypath/absolute_key_path.go:230`<br>`func KeyspaceIDPath(name string) string` | 6 | `pkg/storage/endpoint/keyspace.go:91`；`pkg/keyspace/keyspace.go:361`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:235`<br>`func KeyspaceGroupIDPrefix() string` | 6 | `pkg/tso/keyspace_group_manager.go:598`；`pkg/storage/endpoint/tso_keyspace_group.go:217`；`server/microservice_cleanup.go:161`；`pkg/keyspace/tso_keyspace_group.go:656`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:240`<br>`func KeyspaceGroupIDPath(id uint32) string` | 6 | `pkg/storage/endpoint/tso_keyspace_group.go:180`；`server/microservice_cleanup.go:129`；测试 1 文件 |
| `pkg/utils/keypath/absolute_key_path.go:245`<br>`func GetCompiledKeyspaceGroupIDRegexp() *regexp.Regexp` | 6 | `pkg/tso/keyspace_group_manager.go:439`；`pkg/keyspace/tso_keyspace_group.go:603`；测试 1 文件 |
| `pkg/utils/keypath/absolute_key_path.go:250`<br>`func ServiceMiddlewarePath() string` | 3 | `pkg/storage/endpoint/service_middleware.go:34`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:255`<br>`func StoreLeaderWeightPath(storeID uint64) string` | 3 | `pkg/storage/endpoint/meta.go:90`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:260`<br>`func StoreRegionWeightPath(storeID uint64) string` | 3 | `pkg/storage/endpoint/meta.go:94`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:265`<br>`func ReplicationModePath(mode string) string` | 3 | `pkg/storage/endpoint/replication_status.go:34`；测试 0 文件 |
| `pkg/utils/keypath/absolute_key_path.go:270`<br>`func TimestampPath(groupID uint32) string` | 6 | `pkg/storage/endpoint/tso.go:48`；`tools/pd-backup/pdbackup/backup.go:80`；测试 2 文件 |
| `pkg/utils/keypath/absolute_key_path.go:278`<br>`func RegionPath(regionID uint64) string` | 2 | `pkg/storage/region_storage.go:66`；`pkg/storage/endpoint/meta.go:161`；`server/server.go:837`；测试 2 文件 |
| `pkg/utils/keypath/absolute_key_path.go:313`<br>`func MetaServiceGroupStatusPath(groupID string) string` | 6 | `pkg/storage/endpoint/meta_service_group.go:42`；测试 0 文件 |
| `pkg/utils/keypath/affinity.go:23`<br>`func AffinityGroupPath(groupID string) string` | 5 | `pkg/storage/endpoint/affinity.go:44`；测试 1 文件 |
| `pkg/utils/keypath/affinity.go:30`<br>`func AffinityGroupsPrefix() string` | 5 | `pkg/mcs/scheduling/server/affinity/watcher.go:99`；`pkg/storage/endpoint/affinity.go:61`；测试 0 文件 |
| `pkg/utils/keypath/cluster_id.go:24`<br>`func ClusterID() uint64` | 2（显式 cluster context；不移植全局 setter/resetter） | `pkg/mcs/router/server/grpc_service.go:140`；`pkg/mcs/router/server/sync.go:367`；`pkg/mcs/tso/server/grpc_service.go:100`；`pkg/mcs/tso/server/server.go:304`；`pkg/mcs/metastorage/server/grpc_service.go:160`；`pkg/mcs/scheduling/server/grpc_service.go:506`；`pkg/mcs/scheduling/server/cluster.go:420`；`pkg/syncer/client.go:80`；`pkg/syncer/server.go:334`；`pkg/utils/grpcutil/cluster.go:270`；`pkg/storage/endpoint/cluster_id.go:61`；`tests/cluster.go:300`；`server/grpc_service.go:411`；`server/util.go:62`；`server/server.go:793`；`server/forward.go:525`；`server/api/member.go:75`；`server/cluster/cluster.go:2411`；`pkg/schedule/hbstream/heartbeat_streams.go:97`；测试 12 文件 |
| `pkg/utils/keypath/cluster_id.go:33`<br>`func SetClusterID(id uint64)` | 2（显式 cluster context；不移植全局 setter/resetter） | `pkg/storage/endpoint/cluster_id.go:54`；测试 4 文件 |
| `pkg/utils/keypath/cluster_id.go:38`<br>`func ResetClusterID()` | 2（显式 cluster context；不移植全局 setter/resetter） | 无包外生产直接引用；测试 3 文件 |
| `pkg/utils/keypath/config.go:20`<br>`func ConfigPath() string` | 3 | `pkg/mcs/scheduling/server/config/watcher.go:170`；`pkg/storage/endpoint/config.go:43`；测试 0 文件 |
| `pkg/utils/keypath/config.go:25`<br>`func SchedulerConfigPathPrefix() string` | 3 | `pkg/mcs/scheduling/server/config/watcher.go:108`；`pkg/storage/endpoint/config.go:61`；测试 0 文件 |
| `pkg/utils/keypath/config.go:30`<br>`func SchedulerConfigPath(schedulerName string) string` | 3 | `pkg/storage/endpoint/config.go:71`；测试 0 文件 |
| `pkg/utils/keypath/election.go:24`<br>`func ElectionPath(p *MsParam) string` | 3 | `pkg/id/id.go:168`；`pkg/tso/keyspace_group_manager.go:1507`；`pkg/member/participant.go:183`；`pkg/member/member.go:168`；`server/server.go:2459`；`pkg/keyspace/tso_keyspace_group.go:2261`；测试 3 文件 |
| `pkg/utils/keypath/election.go:38`<br>`func ExpectedPrimaryPath(p *MsParam) string` | 3 | `pkg/mcs/utils/expected_primary.go:43`；测试 1 文件 |
| `pkg/utils/keypath/election.go:49`<br>`func MemberLeaderPriorityPath(id uint64) string` | 3 | `pkg/member/member.go:401`；测试 0 文件 |
| `pkg/utils/keypath/gc_states.go:24`<br>`func GCStateRevisionPath() string` | 4 | `pkg/storage/endpoint/gc_states.go:456`；测试 1 文件 |
| `pkg/utils/keypath/gc_states.go:30`<br>`func GCSafePointPath(keyspaceID uint32) string` | 4 | `pkg/storage/endpoint/gc_states.go:286`；`pkg/storage/endpoint/gc_state_batch.go:61`；测试 2 文件 |
| `pkg/utils/keypath/gc_states.go:38`<br>`func TxnSafePointPath(keyspaceID uint32) string` | 4 | `pkg/storage/endpoint/gc_states.go:338`；`pkg/storage/endpoint/gc_state_batch.go:60`；测试 4 文件 |
| `pkg/utils/keypath/gc_states.go:46`<br>`func GCBarrierPrefix(keyspaceID uint32) string` | 4 | `pkg/storage/endpoint/gc_states.go:371`；测试 0 文件 |
| `pkg/utils/keypath/gc_states.go:51`<br>`func GCBarrierPath(keyspaceID uint32, barrierID string) string` | 4 | `pkg/storage/endpoint/gc_states.go:359`；测试 0 文件 |
| `pkg/utils/keypath/gc_states.go:59`<br>`func GlobalGCBarrierPrefix() string` | 4 | `pkg/storage/endpoint/gc_states.go:399`；测试 1 文件 |
| `pkg/utils/keypath/gc_states.go:64`<br>`func GlobalGCBarrierPath(barrierID string) string` | 4 | `pkg/storage/endpoint/gc_states.go:389`；测试 1 文件 |
| `pkg/utils/keypath/gc_states.go:70`<br>`func ServiceGCSafePointPrefix() string` | 4 | 无包外生产直接引用；测试 0 文件 |
| `pkg/utils/keypath/gc_states.go:76`<br>`func ServiceGCSafePointPath(serviceID string) string` | 4 | 无包外生产直接引用；测试 2 文件 |
| `pkg/utils/keypath/gc_states.go:81`<br>`func CompatibleTiDBMinStartTSPrefix(keyspaceID uint32) string` | 4 | `pkg/storage/endpoint/gc_states.go:414`；测试 2 文件 |
| `pkg/utils/keypath/gc_states.go:89`<br>`func GCSafePointV2Path(keyspaceID uint32) string` | 4 | 无包外生产直接引用；测试 0 文件 |
| `pkg/utils/keypath/gc_states.go:94`<br>`func ServiceSafePointV2Path(keyspaceID uint32, serviceID string) string` | 4 | 无包外生产直接引用；测试 1 文件 |
| `pkg/utils/keypath/gc_states.go:100`<br>`func ServiceSafePointV2Prefix(keyspaceID uint32) string` | 4 | 无包外生产直接引用；测试 0 文件 |
| `pkg/utils/keypath/gc_states.go:105`<br>`func GCSafePointV2Prefix() string` | 4 | 无包外生产直接引用；测试 0 文件 |
| `pkg/utils/keypath/maintenance.go:22`<br>`func MaintenanceTaskPath(taskType string) string` | 7 | `pkg/storage/endpoint/maintenance.go:66`；`server/apiv2/handlers/maintenance.go:117`；测试 1 文件 |
| `pkg/utils/keypath/maintenance.go:27`<br>`func MaintenanceTaskPathPrefix() string` | 7 | `pkg/storage/endpoint/maintenance.go:83`；测试 0 文件 |
| `pkg/utils/keypath/member.go:20`<br>`func MemberBinaryDeployPath(id uint64) string` | 3 | `pkg/member/member.go:446`；测试 0 文件 |
| `pkg/utils/keypath/member.go:25`<br>`func MemberGitHashPath(id uint64) string` | 3 | `pkg/member/member.go:491`；测试 0 文件 |
| `pkg/utils/keypath/member.go:30`<br>`func MemberBinaryVersionPath(id uint64) string` | 3 | `pkg/member/member.go:478`；测试 0 文件 |
| `pkg/utils/keypath/resource_group.go:26`<br>`func ControllerConfigPath() string` | 5 | `pkg/mcs/resourcemanager/server/metadata_watcher.go:39`；`pkg/storage/endpoint/resource_group.go:107`；`server/grpc_service.go:2516`；测试 1 文件 |
| `pkg/utils/keypath/resource_group.go:41`<br>`func ResourceGroupSettingPrefix() string` | 5 | `pkg/mcs/resourcemanager/server/metadata_watcher.go:41`；`pkg/storage/endpoint/resource_group.go:59`；测试 0 文件 |
| `pkg/utils/keypath/resource_group.go:46`<br>`func ResourceGroupStatePrefix() string` | 5 | `pkg/mcs/resourcemanager/server/metadata_watcher.go:43`；`pkg/storage/endpoint/resource_group.go:88`；测试 0 文件 |
| `pkg/utils/keypath/resource_group.go:51`<br>`func ResourceGroupPrefix() string` | 5 | `pkg/mcs/resourcemanager/server/metadata_watcher.go:34`；测试 0 文件 |
| `pkg/utils/keypath/resource_group.go:56`<br>`func KeyspaceResourceGroupSettingPath(keyspaceID uint32, groupName string) string` | 5 | `pkg/mcs/resourcemanager/server/keyspace_manager.go:310`；`pkg/storage/endpoint/resource_group.go:49`；测试 4 文件 |
| `pkg/utils/keypath/resource_group.go:64`<br>`func KeyspaceResourceGroupStatePath(keyspaceID uint32, groupName string) string` | 5 | `pkg/mcs/resourcemanager/server/keyspace_manager.go:313`；`pkg/storage/endpoint/resource_group.go:78`；测试 2 文件 |
| `pkg/utils/keypath/resource_group.go:72`<br>`func KeyspaceResourceGroupSettingPrefix() string` | 5 | `pkg/mcs/resourcemanager/server/metadata_watcher.go:41`；`pkg/storage/endpoint/resource_group.go:65`；测试 0 文件 |
| `pkg/utils/keypath/resource_group.go:77`<br>`func KeyspaceResourceGroupStatePrefix() string` | 5 | `pkg/mcs/resourcemanager/server/metadata_watcher.go:43`；`pkg/storage/endpoint/resource_group.go:94`；测试 0 文件 |
| `pkg/utils/keypath/resource_group.go:82`<br>`func KeyspaceServiceLimitPath(keyspaceID uint32) string` | 5 | `pkg/storage/endpoint/resource_group.go:117`；测试 0 文件 |
| `pkg/utils/keypath/resource_group.go:87`<br>`func KeyspaceServiceLimitPrefix() string` | 5 | `pkg/mcs/resourcemanager/server/metadata_watcher.go:45`；`pkg/storage/endpoint/resource_group.go:131`；测试 0 文件 |
| `pkg/utils/keypath/resource_group.go:92`<br>`func ParseKeyspaceResourceGroupPath(path string) (uint32, string, error)` | 5 | `pkg/mcs/resourcemanager/server/metadata_watcher.go:117`；`pkg/storage/endpoint/resource_group.go:67`；测试 0 文件 |
| `pkg/utils/keypath/rule.go:22`<br>`func RulesPathPrefix() string` | 5 | `pkg/mcs/scheduling/server/rule/watcher.go:107`；`pkg/storage/endpoint/rule.go:94`；测试 1 文件 |
| `pkg/utils/keypath/rule.go:27`<br>`func RuleKeyPath(ruleKey string) string` | 5 | `pkg/storage/endpoint/rule.go:49`；测试 1 文件 |
| `pkg/utils/keypath/rule.go:32`<br>`func RuleCommonPathPrefix() string` | 5 | `pkg/mcs/scheduling/server/rule/watcher.go:543`；测试 1 文件 |
| `pkg/utils/keypath/rule.go:37`<br>`func RuleGroupIDPath(groupID string) string` | 5 | `pkg/storage/endpoint/rule.go:64`；测试 1 文件 |
| `pkg/utils/keypath/rule.go:42`<br>`func RegionLabelKeyPath(ruleKey string) string` | 5 | `pkg/mcs/scheduling/server/affinity/watcher.go:148`；`pkg/storage/endpoint/rule.go:79`；测试 2 文件 |
| `pkg/utils/keypath/rule.go:47`<br>`func RuleGroupPathPrefix() string` | 5 | `pkg/mcs/scheduling/server/rule/watcher.go:108`；`pkg/storage/endpoint/rule.go:59`；测试 1 文件 |
| `pkg/utils/keypath/rule.go:52`<br>`func RegionLabelPathPrefix() string` | 5 | `pkg/mcs/scheduling/server/rule/watcher.go:109`；`pkg/mcs/scheduling/server/affinity/watcher.go:139`；`pkg/storage/endpoint/rule.go:74`；测试 1 文件 |

合计 **76** 个公开生产 func/type 声明；所有签名均为单行，已逐文件阅读函数体。

## key/value、事务及 watch/lease 责任

keypath 自身不读写、不创建事务、不 watch、不申请 lease；只有字符串与解析函数。
以下是调用源码事实，值编码不应混进 canonical keypath crate。

| 键族 | 值与操作依据 | 覆盖归属 |
| --- | --- | --- |
| cluster_id、alloc_id | `pkg/storage/endpoint/cluster_id.go` 用 Uint64ToBytes/BytesToUint64（8 字节大端），create-revision CAS；`pkg/id/id.go` 的分段 ID 预留及 leader compare | 路径 2；值与原子预留归上层 endpoint/ID |
| raft、store、region | `pkg/storage/endpoint/meta.go` protobuf 编码；store weights 用浮点字符串，region 范围按 20 位键排序；bootstrap time 在 `server/server.go` 编码写入 | 路径 2/3；proto 适配和扫描归 endpoint/storage |
| config、scheduler、middleware、replication | `pkg/storage/endpoint/config.go` JSON 配置、scheduler 原始字符串；`service_middleware.go`/`replication_status.go` JSON；成员 deploy/hash/version/priority 在 `pkg/member/member.go` | 路径 3；完整配置及装配归上层主题 |
| GC/txn/barrier | `pkg/storage/endpoint/gc_states.go`：统一 safe point 十六进制；keyspace safe point JSON `{keyspace_id,safe_point}`；txn safe point 十进制；barrier JSON；revision 十进制且以旧 value/不存在比较原子提交整批操作。`gc_state_batch.go` 批量读取 | 路径 4；编码、事务冲突及过期语义归 endpoint/GC |
| rule、label、affinity | `pkg/storage/endpoint/rule.go`、`affinity.go` 的 JSON 与外传 kv.Txn；`pkg/mcs/scheduling/server/rule/watcher.go` 用共同 rule prefix 和 revision watcher | 路径 5；txn/watch 消费归 endpoint/MCS |
| resource group/controller/service limit | `pkg/storage/endpoint/resource_group.go` settings 用 proto；states/controller/service limit 用 JSON；遍历分别读 legacy 与 keyspace prefix 后解析相对后缀 | 路径 5；相对键在所选 KV 实现中使用，不擅加 cluster 前缀 |
| keyspace、group、status | `pkg/storage/endpoint/keyspace.go` 元数据 proto、name→ID 十进制，kv.Txn 组合修改并可用 RevisionTxn 读取；`tso_keyspace_group.go` JSON 与范围读；`meta_service_group.go` 事务保存状态 | 路径 6；业务 schema/txn 归 endpoint |
| timestamp/min-resolved/external | `pkg/storage/endpoint/tso.go` 的 UnixNano 8 字节大端时间窗口与事务、`min_resolved_ts.go`/`external_timestamp.go` 十六进制字符串；`pkg/tso` 负责单调性 | 路径 6；持久化窗口和故障恢复归 TSO/endpoint |
| election/expected primary | `pkg/election/leadership.go` campaign create-revision compare + leased put，watch 从 revision 监听 leader；`pkg/mcs/utils/expected_primary.go` transient flag 带 TTL lease、compare、成功后清理/revoke | 路径 3；lease/fencing/转移归上层 election/MCS |
| registry/service | `pkg/mcs/discovery/register.go` 接受 serializedValue，WithLease 注册、KeepAlive 和地址竞争 CAS；`discover.go` 读服务 prefix | 路径 7；租约生命周期和发现归上层 MCS |
| 恢复与 maintenance | `server/server.go` Recovering/Pitr marker 以存在性判断；`pkg/storage/endpoint/maintenance.go` JSON task、锁键、事务与 prefix + `\xff` 范围 | 路径 7；锁、状态机与 API 归 endpoint/服务装配 |

常量的直接消费者补充：`ClusterIDPath` 在 `pkg/storage/endpoint/cluster_id.go`；
`GCWorkerServiceSafePointID` 在 `pkg/gc/gc_state_manager.go` 和 `pkg/gc/metrics.go`；`NativeBRServiceSafePointID` 在 `pkg/gc/gc_state_manager.go`。
这些是固定字节契约，后两者均归 4，不是持久化键构造器。

## 已有 Rust 能力、硬编码及缺口

当前 `pkg/utils/keypath/` 无 Rust 文件、Cargo.toml 或 crate 注册；
`astersql-pd-keypath` 及任务 2–7 所列 `*_paths.rs`/`*_test.rs` 均为拟建。
现有 Rust 键 schema 硬编码位置如下；HTTP `/pd/api/v1/*` 是路由，不算存储键。

| 当前源码 | 硬编码键与已有测试 | 后续责任 |
| --- | --- | --- |
| `pkg/runtime/node.rs:21` | `/pd/cluster_id`；bootstrap_config 用 create revision CAS 和大端 u64；`pkg/runtime/node_test.rs` 已注册生命周期/失败回收测试 | 路径 2；宿主调用接线归上层 runtime |
| `pkg/member/member.rs:50` | `/pd/{cluster_id}/rust/members/{member_id}`，Rust 自有身份记录，无 Go keypath 等价 API | 上层 member/启动装配盘点，不能伪称 member Go 元信息已迁移 |
| `pkg/member/member.rs:83` | `/pd/C/leader`，value 由 adapter 提供、lease 选主；`member_test.rs` bootstrap/lease loss/natural expiry，Cargo.toml 显式 etcd_behavior target | 路径 3；lease/fencing 保持 member/MetaStore 责任 |
| `pkg/id/allocator.rs:48` | `/pd/C/alloc_id`；`allocator_test.rs` concurrency/restart/fencing、corrupt/exhausted，etcd_behavior target | 路径 2；不在纯键任务重写分配器 |
| `pkg/tso/oracle.rs:98` | `/pd/C/timestamp`；oracle_test/restart_test 显式注册，etcd_behavior target 包括 WAL/进程重启、lease expiry | 路径 6；完整 MCS group TSO 留上层 |
| `pkg/id/allocator_test.rs`、`pkg/member/member_test.rs`、`pkg/runtime/node_test.rs`、`pkg/tso/oracle_test.rs`、`pkg/tso/etcd_behavior_test.rs` | 固定 cluster 的 alloc/leader/timestamp 字节预置与查询；`oracle_test.rs` 的动态 `/pd/{cluster}/timestamp` 亦为现有硬编码 | 对应 2/3/6 的独立值测试，不能以旧测试代替新 schema 全覆盖 |

现有 storage `meta_store_test.rs` 将 9 项 contract 场景注册到 MemoryStore，
`etcd_store_test.rs` 为真实后端 target 并复用 contract_test.rs；覆盖 CAS、revision、
scan、watch replay/cancel/compaction、lease expiry/renew/revoke、fencing 等底座。
这些不含 Go keypath 全集，不能据此宣称 endpoint/schema 等价。
工厂 `pkg/runtime/node.rs` 接收同一个 MetaStore，装配 Member→Allocator→Oracle；
不引入第二套 MetaStore 或私有运行时。当前 storage crate 尚非 kv 下沉后布局。
本轮没有 Rust 新测试、没有新增测试运行数，也没有声称现有测试刚刚通过。

## Go 断言与未覆盖原因

- `pkg/utils/keypath/region_test.go::TestRegionPath`：1000 个 rand.Uint64 输入，
  断言 `RegionPath(id) == fmt.Sprintf("/pd/0/raft/r/%020d", id)`；同文件 benchmark
  不算行为测试。此测试没有显式验证非零 cluster、多租户、错误输入。
- `pkg/utils/keypath/resource_group_test.go::TestParseKeyspaceResourceGroupPath`：
  8 个 table case：123/group1、456/group1/subgroup、0/group1、uint32 最大值成功；
  空串、无组分隔符、abc、超 uint32 失败。TestMain 运行 goleak。
  没有现成断言覆盖空尾、重复 slash 与错误文本，本轮只作源码推导。
- `pkg/storage/endpoint/cluster_id_test.go` 检查 cluster ID 初始化与重复读取；
  `gc_states_test.go`、`gc_state_batch_test.go`、`tso_keyspace_group_test.go` 是相邻
  业务测试。对应生产包其他行为由调用方测试覆盖，不等于 keypath 值全集已测试。
- task 2–7 实施时须按各任务执行 Go oracle 和 Rust 红绿；当前缺 crate/纯值测试、
  无全量实际跨语言字节差分，原因是本任务明确为文档盘点。缺口全部归上述任务或
  上层领域主题，不在本轮用编译、mock 或历史测试结果代替。

## 本轮验证

公开声明扫描的实际命令（退出码 0）：

```sh
rg -n '^(func |type |pub |    pub )' pkg/utils/keypath/absolute_key_path.go pkg/utils/keypath
```

| 确切命令/检查 | 结果 |
| --- | --- |
| 下方公开声明扫描命令 | 退出 0；重复指定 absolute 文件的结果去重，包含测试和未导出声明，逐文件人工核对得到 76 项公开 func/type |
| `git diff 5d5948052a9ee9e3fa7a09145fe2916a2912c223 -- pkg/utils/keypath` | 退出 0、无输出，Go oracle 自规划基准未漂移 |
| Python 标准库全 Go import/别名与符号扫描、来源 SHA-256、公开声明覆盖校验 | 退出 0；12 来源文件与 76 个声明，全部映射责任 |
| 引用存在性检查（文档中已存在来源文件与 Markdown 链接，拟建项单独标注） | 退出 0；没有把拟建 canonical crate 作为准入证据 |
| `git diff --check` | 退出 0，无空白错误 |

已审查差异只有本证据文档和任务状态/清理；未修改 Go/Rust、manifest、锁文件、
只读 plan.md 或来源主题。其他会话的元数据观测任务状态保留原样、不纳入提交。

## 任务 2：集群与元数据键（2026-10-23）

准入：批次 1 提交 `cf27d67306a0a9c4182a68825e0247b70d6fdd65` 已存在，
前文盘点及 core 基线证据齐全；`git diff 5d5948052 -- pkg/utils/keypath`
为空。无更深层 AGENTS.md。只读总计划与来源主题未修改。

新增 canonical crate `astersql-pd-keypath`，默认 features，无第三方依赖。
`ClusterPaths::new(u64)` 保存显式、不可变集群上下文，`cluster_id()` 取回 ID；
不引入 Go 全局 setter/resetter。`CLUSTER_ID_PATH` 固定 `/pd/cluster_id`。
本轮方法：`cluster_path`、`cluster_bootstrap_time_path`、`alloc_id_path`、
`keyspace_alloc_id_path`、`store_path_prefix`、`store_path`、`region_path`、
`extract_store_id_from_path`。配置/选主、GC、规则、其他键空间路径留给后续任务。
纯值路径无存储行为，因此本任务无需 MemoryStore/etcd 两个后端入口。

### Go 依据及实际差分

- 来源：`absolute_key_path.go` 的 `ClusterIDPath`、`AllocIDPath`、
  `KeyspaceAllocIDPath`、`ClusterPath`、`ClusterBootstrapTimePath`、
  `StorePath`、`StorePathPrefix`、`ExtractStoreIDFromPath`、`RegionPath`；
  `cluster_id.go` 的默认 0 行为。
- 已有测试：`TestRegionPath` 对 1000 个随机 uint64 比较 `%020d`；
  相邻 `TestParseKeyspaceResourceGroupPath` 的 8 个子场景也随包测试通过。
- 真实 Go 输出工具：[cluster_paths.go](../../scripts/rust-theme3-oracle/cluster_paths.go)，
  [main.go](../../scripts/rust-theme3-oracle/main.go) 仅分发。
  `go run ./scripts/rust-theme3-oracle cluster_paths`（退出 0）输出 70 条 TSV。
  原样保存在独立 [cluster_paths_test.rs](../../pkg/utils/keypath/cluster_paths_test.rs)
  的 `GO_ORACLE` 常量，Rust 逐条调用真实函数断言字节/错误类别，并输出已核对记录。
- cluster 输入 `[0, 42, 18446744073709551615]`；store/region 输入
  `[0, 1, 42, 9999999999999999999, 10000000000000000000, 18446744073709551615]`。
  52 条路径记录：固定键 1 条、每个 cluster 的非 ID 路径 5 条、store/region 各 6 条。
  例如 `/pd/0/raft/s/00000000000000000000`、
  `/pd/42/raft/r/09999999999999999999`、
  `/pd/18446744073709551615/raft/r/18446744073709551615`。
- 18 条解析输入及结果完整保存在上述 Go 工具和 Rust oracle 常量。
  覆盖空串/全零、裸数字/前导零、u64 最大值/溢出、正负号/空格/下划线/
  Unicode/非法尾字符、当前与其他集群完整键。先剥当前 prefix，再去全部前导零；
  零值构造有效但反解为空串而失败；裸 `123` 成功、错误集群完整键失败。
  `18446744073709551616x` 在非法尾字符之前先溢出，返回 range。
  Rust `StoreIdError` 保留 syntax/range 类别及底层错误短语，通过 `Result`
  表达失败，不复刻 Go 的 `NumError` 包装文本或失败时应忽略的部分数值。
- 将 Go TSV 和 Rust 测试 stdout 中 `ORACLE` 行排序逐行比较，70/70 完全一致；
  键路径比较用 `as_bytes()`，没有归一化。这是本轮实际运行差分，非仅源码推导。

### 测试驱动与验证

每次 Cargo 阶段原子 `mkdir target/rust-slot-locks/slot-N.lock` 从 1–10 领取，
两阶段均实际获得槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划、任务、会话及阶段 shell PID（84452 / 84613）；使用 EXIT/INT/TERM
清理，全部 Cargo/fixture 子进程结束后释放锁，缓存保留。

- `make gotest GOTEST_ARGS='./pkg/utils/keypath -count=1'`：退出 0，
  2 个顶层测试（其中资源组测试 8 个子场景），包输出 `ok ... 0.242s`；
  make 已完成 failpoint 禁用，后续检查无 Go 生产差异或生成 failpoint 文件。
- `cargo fmt --all`：退出 0；新增 crate 后 `cargo check --workspace`：退出 0，
  Cargo.lock 仅新增无依赖的 keypath 包条目。
- 红灯：`cargo test --locked -p astersql-pd-keypath --lib cluster_paths -- --nocapture`：
  退出 **101**，1 passed / 2 failed。最小实现尚未补零，实际断言
  `/pd/0/raft/s/0` 不等于 `/pd/0/raft/s/00000000000000000000`，
  隔离测试也因 `/pd/1/raft/s/42` 不等于 20 位键失败。无缺符号/环境错误替代红灯。
- 补齐 store/region 的 `:020` 后 `cargo fmt --all`：退出 0；
  **同一命令、同一断言**绿灯：退出 **0**，3 passed / 0 failed / 0 ignored，
  其中 52 条路径断言、18 条解析断言和独立集群上下文断言全部通过。
- `cargo metadata --format-version 1 --no-deps`：退出 0，包含新 keypath crate。
- `cargo fmt --all -- --check`：退出 0。
- `cargo check --workspace --locked`：退出 0；仅有既有 server 字段废弃警告。
- `cargo test --workspace --locked`：退出 0，**449 passed / 0 failed / 1 ignored**，
  包含 2 个 syncutil compile-fail doctest；既有 ignored 是需单独二进制的
  `tests/rust-compat/tso_test.rs` 测试，本任务未新增 ignored。
- `git diff --check`：退出 0。审查确认无 Go 生产文件变化、无无关 Rust 格式化差异；
  仅 workspace 注册、必要锁文件条目、本轮 crate/测试、Go oracle 及本文证据。
  槽位锁目录最终为空，保留 `target/rust-slot-1` 缓存。

结论：任务 2 已完成；独立测试显式注册于 `pkg/utils/keypath/lib.rs`。
本结论仅覆盖本轮键路径行为，不代表整个 Go keypath 包已迁移。

## 任务 3：配置与成员键（2026-10-23）

准入：批次 2 提交 `6c1de2263` 及前节完整红绿灯、449 项 workspace
通过证据已核验，canonical `astersql-pd-keypath` 存在。相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223`，本任务涉及的 Go keypath
源码无差异；仅根 AGENTS.md，无更深覆盖。总计划和来源主题保持只读。

### 实现与 Go 依据

在 canonical crate 新增 [member_config_paths.rs](../../pkg/utils/keypath/member_config_paths.rs)，
独立 [member_config_paths_test.rs](../../pkg/utils/keypath/member_config_paths_test.rs)
已在 `lib.rs` 显式注册。复用 `ClusterPaths` 显式不可变集群上下文，无全局 ID、
新依赖、features、Cargo manifest/lock 变更或服务/存储算法。

- `config.go`：`ConfigPath`、`SchedulerConfigPathPrefix`、`SchedulerConfigPath`。
- `member.go`：`MemberBinaryDeployPath`、`MemberGitHashPath`、`MemberBinaryVersionPath`。
- `election.go`：`ElectionPath`、`ExpectedPrimaryPath`、`MemberLeaderPriorityPath`。
- `absolute_key_path.go`：`MsParam`、`Prefix`、`ServiceMiddlewarePath`、
  `StoreLeaderWeightPath`、`StoreRegionWeightPath`、`ReplicationModePath` 及 schema。
  Rust 对应 snake_case 公开函数/方法；`MsParam` 用 String/u32 保留字段含义。
- 真实实现是 `path.Dir`（不是任务描述中的 Join）；Rust `prefix` 仅按 `/`
  分段，清理父目录中的空段、`.`、`..`，保留相对上溯并限制绝对路径上溯到根。
  其余构造器直接格式化，空名称、嵌套名称、斜杠和 `..` 不清理、不验证。
- `election_path(None)` 或空服务名返回 PD leader；精确 `tso` 名称才采用
  TSO schema：组 0 用 `00000`，其他组最小宽度 5、超宽不截断。
  其他服务忽略 group。`expected_primary_path(&MsParam)` 要求有效引用，
  在类型层排除 Go nil 解引用 panic；空服务名仍返回 `/ms/C//primary/expected_primary`，
  不误用 election 的 PD leader 分支。
- 成员 ID 为普通十进制，store weight ID 为最小 20 位十进制；priority 是键名，
  本任务不迁移优先级值/选举行为。纯键路径无存储副作用，无需强造双后端入口。

### 真实 Go 输出与跨语言差分

新增场景 [member_config_paths.go](../../scripts/rust-theme3-oracle/member_config_paths.go)，
`main.go` 只新增分发。`go run ./scripts/rust-theme3-oracle member_config_paths`
退出 **0**，产生 **263** 条 TSV，原样固定在独立测试 `GO_ORACLE` 常量中。
Rust 调用真实公开函数，逐条 `as_bytes()` 断言并输出实际值；将 Go TSV 与
Rust stdout 的 ORACLE 行排序比较，**263/263 完全一致**，比较脚本退出 0。
没有忽略或归一化结果，以下输入完整保留在工具与测试中：

- cluster `[0, 42, u64::MAX]`；名称 `""`、`balance-region`、`nested/name`、
  `/a//../b/`、`调度`；成员/store ID `[0, 1, u64::MAX]`。
- service `""`、`tso`、`scheduling`、`tso/`、`/x//../y/`，分别组合 group
  `[0, 1, 99999, 100000, u32::MAX]`；每个 cluster 额外覆盖 nil election。
- 每个 cluster 82 条键输出（合计 246），另有 17 条 Prefix 输入，包含空路径、
  根/重复 slash、尾 slash、`.`/`..`、相对上溯、绝对越根、反斜杠和 Unicode。
  例如 Prefix(`a/../b`) 为 `.`，Prefix(`a/..`) 为 `a`；
  非清理构造器保留 `/pd/42/scheduler_config//a//../b/`。

### 测试驱动及验证记录

Go oracle 首先串行执行：
`make gotest GOTEST_ARGS='./pkg/utils/keypath -count=1'`，退出 **0**，
2 个顶层测试（`TestRegionPath` 比较 1000 个随机 ID，资源组测试含 8 个子场景），
包输出 `ok ... 0.237s`。等待 make 完成 failpoint disable 后才编辑和运行 Rust；
检查无生成 failpoint 文件，无 Go 生产变更。

两个 Cargo 阶段均从 1–10 顺序原子 mkdir 动态获得 **槽位 1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划/任务/会话和 shell PID（红灯 **88339**，绿灯/基线 **88435**）；
EXIT/INT/TERM trap 清理，各阶段所有子进程结束后释放自己的锁并保留缓存。
最终 `target/rust-slot-locks` 为空。

| 命令 | 退出码与有效结果 |
| --- | --- |
| `cargo fmt --all`（红灯、绿灯阶段各一次） | 均 0，无其他 Rust 文件格式变化 |
| `cargo test --locked -p astersql-pd-keypath --lib member_config_paths -- --nocapture`（红灯） | **101**；1 passed / 1 failed。真实断言：TSO group 1 实际 `/ms/0/tso/keyspace_groups/election/1/primary`，Go 期望 `/ms/0/tso/keyspace_groups/election/00001/primary`；不是编译或环境失败 |
| 同一聚焦命令（修复最小 5 位补零后） | **0**；**2 passed / 0 failed / 0 ignored**；263 条 Go 字节断言及独立集群上下文断言通过，测试未削弱 |
| `cargo metadata --format-version 1 --no-deps` | 0；canonical crate 存在，无新增依赖 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0；仅既有 server 废弃字段警告 |
| `cargo test --workspace --locked` | **0**；**451 passed / 0 failed / 1 ignored**，含两个 syncutil compile-fail doctest；既有 ignored 为需单独二进制的 TSO 测试，本轮未新增 ignored |
| `git diff --check` | 0 |

审查：本轮仅生产/独立测试模块、Go oracle 场景及分发、本文和任务清理。
无 Go 生产、依赖、manifest/lock 或只读计划变化；其他会话的元数据观测任务状态
保留原样，不纳入提交。任务 3 已完成，本结论限于配置/成员/选主键值构造。
