# Response DTO 与错误边界契约

审计日期：2026-10-23。任务 62，依赖批次 61。Go 基准
`0820f0b67b0e49b289b84fdf66b7b8fb178afe82`；当前审计提交
`d28695fde21a49a1976158735866122a45edcb99`。`git diff` 确认基准之后
`pkg/response` 无差异。本文是后续实现清单，不代表 Rust response 已实现。
遵循 [架构](architecture.md)、[移植指南](porting-guide.md) 和
[依赖基线](dependencies.md)。本任务不运行 Cargo、Go 测试或 failpoint 命令。

## 依赖核验与证据边界

- 批次 60 提交 `11238f195` 提供
  [core_store_adapter.rs](../../pkg/server/core_store_adapter.rs) 的
  `store_from_proto/store_to_proto/store_stats_from_proto/store_stats_to_proto`。
- 批次 61 提交 `d28695fde` 提供
  [core_region_adapter.rs](../../pkg/server/core_region_adapter.rs) 的
  `peer_from_proto/peer_to_proto/region_from_proto/region_to_proto/region_from_heartbeat`；
  [测试](../../pkg/server/core_region_adapter_test.rs) 显式注册于
  [lib.rs](../../pkg/server/lib.rs)，有 `core_task_61_metadata_roles`、
  `core_task_61_sizes`、`core_task_61_heartbeat_fields`、`core_task_61_missing_fields`
  四个真实测试函数。提交、实现和断言均存在，未以任务文件删除作为依赖证据。
  本审计只核验源码与提交，不宣称重新运行这四个测试。
- 领域 [RegionInfo](../../pkg/core/region.rs)、
  [RegionStats](../../pkg/core/region_flow.rs)、
  [StoreInfo/StoreMeta](../../pkg/core/store_meta.rs)、
  [StoreStats](../../pkg/core/store_stats.rs)、
  [时间](../../pkg/core/store_time.rs)、
  [leader score](../../pkg/core/store_resource.rs)、
  [region score](../../pkg/core/store_score_v2.rs) 已存在。
  response crate、DTO、JSON 与取消 API 尚不存在，分别归任务 63–67。
- 必须继续核对的窄缺口：Rust heartbeat adapter 只设置 unified-read CPU，未设置
  Go HTTP 使用的 deprecated CPUUsage；任务 64 必须区分这两个计数，必要时补窄适配，
  不能把 read CPU 当成 cpu_usage。Rust `StoreInfo::store_stats()` 总返回值快照，
  未暴露 Go `GetStoreStats()!=nil` 的原始报告存在性；任务 66 必须证明或补齐
  start_ts 是否存在的语义，不能凭零统计推断是否曾报告。这些是后续故事的实现前置，
  不阻塞本次只读契约审计；未宣称对应行为已经覆盖。

## 符号、用途与负责人

Go 来源为 [region.go](../../pkg/response/region.go)、
[store.go](../../pkg/response/store.go)，实际序列化以
[region_easyjson.go](../../pkg/response/region_easyjson.go) 为准。

| 符号 | 公开或内部用途、调用方 | Go 断言依据（本次未运行） | Rust 缺口与任务 |
| --- | --- | --- | --- |
| MetaPeer、PDPeerStats；fromPeer/fromPeerSlice/fromPeerStats/fromPeerStatsSlice | 公开 HTTP DTO；内部构造由 InitRegion 调用 | pkg/response/region_test.go: TestPeer、TestPeerStats，四种 role 的完整 map | core Peer/DownPeer 已有；JSON 与 nil 兼容归 63 |
| 两个 setDefaultIfNil | 内部 easyjson 空嵌入指针防护，由 covertAPIRegionInfo 调用 | server/api/region_test.go: TestRegionsInfoMarshal 的 nil peer/leader/pending/down peer.peer | 63 的表示与 64/65 序列化入口 |
| RegionInfo、ReplicationStatus；fromPBReplicationStatus | 公开区域 DTO、内部状态转换 | TestEasyJSONCompatibility；server/api/region_test.go: TestRegionCPUUsageCompatibility | core 元数据/统计/复制状态已有，DTO 与转换归 64 |
| NewAPIRegionInfo、InitRegion | 公开构造与复用；server/api/region_test.go 直接构造 | TestRegionCPUUsageCompatibility；TestRegionsInfoMarshal | 64，复用边界在 65 |
| RegionInfo.Adjust、RegionsInfo.Adjust | 公开但仅用于测试，恢复 DownPeers 的嵌入 PeerStats.Peer | server API JSON 反序列化比较；不能作为生产转换 | 64/65 的测试规范化；Rust 不需要复制指针别名 |
| RegionsInfo、MarshalRegionsInfoJSON | 公开列表与取消；server/api/region.go 和 scheduling v1 API | TestRegionsInfoMarshal | 65 |
| MarshalRegionInfoJSON、covertAPIRegionInfo（源码拼写） | 公开单项序列化、内部复用/修复；server/api/region.go | TestEasyJSONCompatibility 仅 roundtrip；nil 行为见 TestRegionsInfoMarshal | 64，取消细节 65 |
| MetaStore、SlowTrend、StoreStatus、StoreInfo、StoresInfo、BuildStoreInfo | 公开 Store DTO；server/api/store.go、label.go；pkg/mcs/scheduling/server/apis/v1/api.go | server/api/store_test.go: TestDownState；response 包无 Store 字段逐项测试 | 66 字段与格式，67 状态 |
| DisconnectedName、DownStateName | 公开常量；store.go 及 server/api/store.go 状态过滤 | TestDownState | 67 |
| 五个 DTO 的 MarshalJSON/MarshalEasyJSON/UnmarshalJSON/UnmarshalEasyJSON | MetaPeer、PDPeerStats、RegionInfo、RegionsInfo、ReplicationStatus 的公开生成方法；easyjsonXXEncode/Decode 为内部函数，含 RegionEpoch helper | TestEasyJSONCompatibility（注意 json.Marshal 也会调用生成 MarshalJSON，并非独立编码器差分） | 63–65 用 serde 等表达契约，不移植生成代码 |

调用方补充：`pkg/mcs/scheduling/server/cluster.go` 消费 RegionsInfo。
根 HTTP 与微服务 HTTP 的路由、查询参数和鉴权仍在宿主适配层，任务 63–67 不接入网络。

## 逐字段清单

以下表格从当前 DTO 声明逐字段列出；“必有”指生成 JSON 中保留零值，
“省略”指相应空值省略。所有整数为 JSON 数字，不能改成 protobuf JSON 的字符串。
每行都继承上述符号行的调用方、Go 证据和 Rust 缺口。

### RegionInfo（任务 64）

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| ID | `id` | `uint64` | 必有 |
| StartKey | `start_key` | `string` | 必有 |
| EndKey | `end_key` | `string` | 必有 |
| RegionEpoch | `epoch` | `*metapb.RegionEpoch` | 空值省略 |
| Peers | `peers` | `[]MetaPeer` | 空值省略 |
| Leader | `leader` | `MetaPeer` | 必有对象（生成器忽略 struct 的 omitempty） |
| DownPeers | `down_peers` | `[]PDPeerStats` | 空值省略 |
| PendingPeers | `pending_peers` | `[]MetaPeer` | 空值省略 |
| CPUUsage | `cpu_usage` | `uint64` | 必有 |
| WrittenBytes | `written_bytes` | `uint64` | 必有 |
| ReadBytes | `read_bytes` | `uint64` | 必有 |
| WrittenKeys | `written_keys` | `uint64` | 必有 |
| ReadKeys | `read_keys` | `uint64` | 必有 |
| ApproximateSize | `approximate_size` | `int64` | 必有 |
| ApproximateKeys | `approximate_keys` | `int64` | 必有 |
| ApproximateKvSize | `approximate_kv_size` | `int64` | 必有 |
| ApproximateColumnarKvSize | `approximate_columnar_kv_size` | `int64` | 必有 |
| Buckets | `buckets` | `[]string` | 空值省略 |
| BucketVersion | `bucket_version` | `uint64` | 空值省略 |
| ReplicationStatus | `replication_status` | `*ReplicationStatus` | 空值省略 |

### ReplicationStatus（任务 64）

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| State | `state` | `string` | 必有 |
| StateID | `state_id` | `uint64` | 必有 |

### RegionsInfo（任务 65）

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| Count | `count` | `int` | 必有 |
| Regions | `regions` | `[]RegionInfo` | 必有 |

### SlowTrend（任务 66）

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| CauseValue | `cause_value` | `float64` | 必有 |
| CauseRate | `cause_rate` | `float64` | 必有 |
| ResultValue | `result_value` | `float64` | 必有 |
| ResultRate | `result_rate` | `float64` | 必有 |

### StoreStatus（任务 66）

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| Capacity | `capacity` | `typeutil.ByteSize` | 必有 |
| Available | `available` | `typeutil.ByteSize` | 必有 |
| UsedSize | `used_size` | `typeutil.ByteSize` | 必有 |
| LeaderCount | `leader_count` | `int` | 必有 |
| LeaderWeight | `leader_weight` | `float64` | 必有 |
| LeaderScore | `leader_score` | `float64` | 必有 |
| LeaderSize | `leader_size` | `int64` | 必有 |
| RegionCount | `region_count` | `int` | 必有 |
| RegionWeight | `region_weight` | `float64` | 必有 |
| RegionScore | `region_score` | `float64` | 必有 |
| RegionSize | `region_size` | `int64` | 必有 |
| LearnerCount | `learner_count` | `int` | 空值省略 |
| WitnessCount | `witness_count` | `int` | 空值省略 |
| PendingPeerCount | `pending_peer_count` | `int` | 空值省略 |
| SlowScore | `slow_score` | `uint64` | 空值省略 |
| NetworkSlowScore | `network_slow_score` | `map[uint64]uint64` | 空值省略 |
| SlowTrend | `slow_trend` | `*SlowTrend` | 空值省略 |
| SendingSnapCount | `sending_snap_count` | `uint32` | 空值省略 |
| ReceivingSnapCount | `receiving_snap_count` | `uint32` | 空值省略 |
| IsBusy | `is_busy` | `bool` | 空值省略 |
| StartTS | `start_ts` | `*time.Time` | 空值省略 |
| LastHeartbeatTS | `last_heartbeat_ts` | `*time.Time` | 空值省略 |
| Uptime | `uptime` | `*typeutil.Duration` | 空值省略 |
| PauseLeaderTransferIn | `pause_leader_transfer_in` | `bool` | 空值省略 |
| PauseLeaderTransferOut | `pause_leader_transfer_out` | `bool` | 空值省略 |

### StoreInfo（任务 66）

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| Store | `store` | `*MetaStore` | 必有 |
| Status | `status` | `*StoreStatus` | 必有 |

### StoresInfo（任务 66）

| Go 字段 | JSON key | 类型 | 存在规则 |
| --- | --- | --- | --- |
| Count | `count` | `int` | 必有 |
| Stores | `stores` | `[]*StoreInfo` | 必有 |

### 嵌入字段（任务 63、66）

- MetaPeer：`role_name` 始终存在；`is_learner` 仅 Learner 为 true 时输出，
  DemotingVoter 不算 Learner。嵌入 `id/store_id/role/is_witness` 零值省略。
  role 为数值，role_name 为枚举 String()，未知枚举需要保留其数值字符串。
  nil peer 经 helper 变为 `{"role_name":""}`，与显式零 Peer 的 `"Voter"` 区分。
- PDPeerStats：`peer` 必有；`down_seconds` 为 uint64、零值省略；匿名 PeerStats
  的原始 peer 被显式 MetaPeer 覆盖。nil `PeerStats` 切片元素会先在 fromPeerStats
  解引用而 panic；已有测试覆盖的是非 nil PeerStats 内含 nil peer，不能混称安全。
- MetaStore：`state_name` 必有。嵌入 metapb.Store 的
  `id/address/state/labels/version/peer_address/status_address/git_hash/start_timestamp/`
  `deploy_path/last_heartbeat/physically_destroyed/node_state/txn_protocol_version_range`
  均遵循 Go json omitempty。state/node_state 为数值；labels 元素 `key/value` 空串省略；
  txn_protocol_version_range 存在时 `min/max` 的零值省略。元数据不得误加 Region
  的 encryption/flashback 字段；StoreMeta 已由任务 60 保留对应元数据。

## 转换与边界行为

- InitRegion(nil, s) 返回 nil，不改 s；NewAPIRegionInfo(nil) 返回 nil。
  MarshalRegionInfoJSON(nil region) 并不输出 null，而是对已分配零 DTO 进行默认修复后编码。
  InitRegion 的 s 本身为 nil 且输入非 nil 时会 panic，不发明成功保证。
- start_key/end_key/buckets 使用 HexRegionKeyStr 的大写十六进制，空 key 是空字符串。
  epoch 中 conf_ver/version 为 uint64、零值省略。复制状态 state 是枚举 String()，
  state_id 为数字且零值也输出。近似容量直接使用领域已转换的 MiB，不重复换算。
- InitRegion 每次清空 Buckets，但只在新版本 >0 时写 BucketVersion；复用 DTO 时
  正版本后跟 nil/零版本会保留前一版本。MarshalRegionsInfoJSON 复用同一个 DTO，
  因而任务 65 应固定该已观察源码行为；若修正需另立兼容决策，不静默归一化。
- RegionsInfo 的生成 MarshalJSON 在 nil Regions 时默认输出 null，非 nil 空切片为 []；
  MarshalRegionsInfoJSON 对 nil/空输入都输出 `{"count":0,"regions":[]}`。
  count 为输入长度，含 nil 项；不能过滤输入改变 count。
- 单项在转换前检查一次 ctx.Done；列表在每项前检查，取消返回 nil bytes 与 ctx.Err，
  不返回已写的半截 JSON；空列表即使已经取消仍成功。循环内无额外异步任务或后台 Runner。
- StartTS 仅 GetStoreStats()!=nil 时分配；LastHeartbeatTS 仅非 Go 零时间时分配；
  Uptime 仅 >0 时分配。time.Time 为 Go JSON 时间字符串（含时区和可选小数秒），
  Duration 为 Go time.Duration.String()；ByteSize 用 units.BytesSize(float64(value))
  的人类可读字符串，不能替换成裸 bytes 或另一套单位/舍入规则。network_slow_score
  的 uint64 map key 是十进制对象 key。NaN/Inf 的 JSON 编码错误需由 66 的格式测试确定，
  不把浮点非法值吞成零。SlowTrend 四字段原样复制。

## ScheduleConfig 的窄只读输入

BuildStoreInfo 实际只读五项：LeaderSchedulePolicy（StringToSchedulePolicy 后，delta=0）、
RegionScoreFormulaVersion、HighSpaceRatio、LowSpaceRatio（RegionScore，delta=0）、
MaxStoreDownTime.Duration。任务 66 接收前四项，任务 67 接收阈值与调用方传入的 now。
不迁移 ScheduleConfig、其默认值装配或 schedule 子系统。
[store.go 调用方](../../server/api/store.go) 的列表先取单一配置快照，保证一批 Store
一致；该责任仍在宿主。Up 且 DownTime > MaxStoreDownTime 时为 Down；否则 IsDisconnected
时为 Disconnected；等于阈值不算 Down。Offline/Tombstone 不覆盖；state_name 不改变 state。

## 错误分类与 server 映射

| 类别 | 当前真实行为 | 迁移责任与覆盖边界 |
| --- | --- | --- |
| response 取消、写入错误 | 两个 Marshal 函数返回 ctx.Err 或 out.Error；response 不定义 HTTP 状态 | 64/65 返回可区分错误；宿主决定状态 |
| 参数非法、对象缺失 | server/api/region.go GetRegionByID：ParseUint/零 ID →400，缺失 →404，序列化错误（含取消）→500，成功→200 Data；server/api/store.go Get：解析失败走 ErrorResp InvalidInput，缺失→404，成功→200 JSON | 既有 Go 路由事实，不能一概把取消改写成 408/499；网络迁移不在本计划 |
| 微服务序列化错误 | pkg/mcs/scheduling/server/apis/v1/api.go 使用 500 String 错误，成功 200 Data | 宿主适配，63–67 不修改服务 |
| 领域错误 | core/region_stale.rs 的 StaleRegionError、RegionValidationError::{NotFound,InvalidEpoch}；core/stores_control.rs 的 StoreControlError::{StoreNotFound,SlowTrendEvicted} | core 只保留领域数据/错误；不是 response DTO，也无已实现的通用 HTTP 映射 |
| protobuf 输入缺失 | server/core_region_adapter.rs region_from_heartbeat 返回 missing region / missing down peer 字符串错误 | 61 已有窄适配；不是 HTTP 404，也未接入 region 路由 |
| Rust 现有网络服务 | server/http.rs 仅 health/members，健康失败503；grpc.rs 的 failed_precondition/unavailable/unimplemented/unknown 对应现有身份、ID、TSO 路径 | 不宣称已有 Region/Store HTTP 或领域错误到 gRPC 的完整映射 |

response 包无 failpoint、工厂、指标或 Runner 注册。生成序列化方法的语义由 63–65
覆盖，生成器本身不迁移；Go 对象复用只是性能机制，Rust 仍须覆盖其可观察 JSON。
心跳指标由前序 59 的 server/core_trace.rs 宿主 Registry 负责，非本次 response 工作。

## 验证记录

本次为文档审计，有效运行测试数 **0**（不适用）；源码中的 3 个 response 测试函数和
4 个任务 61 测试函数均仅作断言/依赖证据，不宣称跨语言运行差分。
只读测试来源：[response 测试](../../pkg/response/region_test.go)、
[Region API 测试](../../server/api/region_test.go)、[Store API 测试](../../server/api/store_test.go)。
后续 fixture 必须注明输入、对应源码分支和已有断言，未覆盖场景由 Rust 真行为测试补齐。

- `rg -n '^func |^type |failpoint\.' pkg/response -g '*.go'`：退出 0，核验公开/内部及生成符号。
- `rg -n 'response\.|MarshalRegionsInfoJSON|MarshalRegionInfoJSON|BuildStoreInfo' server pkg --glob '*.go' --glob '!region_easyjson.go'`：退出 0，核验实际调用方。
- `git diff 0820f0b67b0e49b289b84fdf66b7b8fb178afe82 -- pkg/response`：退出 0，无输出。
- `git show --stat d28695fde`：退出 0，核验批次 61 的真实提交。
- Python 路径/字段核验：退出 0，21 个相对链接存在，57 个非嵌入 DTO 字段逐项匹配；
  `git diff --check`：退出 0。拟建 response Rust 文件未伪装成现存链接。
