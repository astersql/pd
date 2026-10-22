# Core 覆盖与边界契约

审计日期：2026-10-22；任务 8，批次 8，依赖批次 7。
Go 基准 `0820f0b67b0e49b289b84fdf66b7b8fb178afe82`；审计 HEAD `d594a0f133deeaf6b5bc4de2dc93915634f7d9be`。
[架构](architecture.md)、[依赖基线](dependencies.md)、[移植指南](porting-guide.md) 为约束。
本文固定迁移责任，不代表尚未实现的 core 能力已经通过测试。任务编号在任务文件删除后仍有效。

## 前序证据与现状

已核对提交 `25ce7bf3c`、`91ccfed97`、`7893989e5`、`85f1142cc`、`a4efa3046`、`d594a0f13`
分别引入任务 2–7 的类型、v1 接纳、重置、单窗口、多优先级窗口、反馈。
批次 7 的 [feedback 实现](../../pkg/core/storelimit/window_feedback.rs) 按 low 可用性决定是否累积误差，
以 `max(100, 20*e+10*last_sum)` 调容且保留 used；[测试](../../pkg/core/storelimit/window_feedback_test.rs)
登记了 `core_task_07_go_feedback_sequence` 与 `core_task_07_all_priority_capacities_and_usage_survive_feedback`。
历史 Go oracle（提交 `d594a0f13` 的 `pkg/core/storelimit/rust_core_task_07_oracle_test.go`，后于 `014342310` 删除） 与 Rust 读取同一
[fixture](../../pkg/core/storelimit/rust-fixtures/core_task_07.json)，[lib](../../pkg/core/storelimit/lib.rs) 已注册测试模块。
这是提交和源码核验，本任务没有重跑前序行为测试，不把任务文件删除当作通过证据。

基准后 `pkg/core` 根包、constant、rangetree 的 Go 生产源码无变化；新增内容仅在 storelimit 的 Rust、fixtures 和 Go oracle。
根包尚无 Cargo manifest/lib，`StoreInfo`、`RegionInfo`、`RegionsInfo`、`BasicCluster` 均未提供 Rust API。
这是任务 9–58 的计划交付，不是本次文档审计的缺失前置依赖。

## 领域和适配边界

领域值自有 Peer/Role、Store 元数据/统计、Region 元数据/epoch/区间；不能直接复用 metapb/pdpb。
任务 9 定义 canonical core crate，10 定义键区间与 epoch；11–27 Store，28–38 Region，39–56 索引，57–58 BasicCluster。
59 在 server 负责显式时钟、阶段追踪和宿主 Registry；60/61 在 server 负责 Store/Region 协议转换。
62–67 是 response 覆盖与 JSON，68 核对公开能力，69 做完整回归；本任务不提前关闭来源主题。

[factory.go](../../pkg/core/factory.go) 的 Region/Peer/Store/StoreStats/PeerStats/TimeInterval/QueryStats 工厂
只为 Go 深拷贝和分配服务，领域 clone 的值与共享语义分别由 16/29 验收，协议生成值构造由 60/61 验收。
RegionHeartbeatResponseFactory 的网络响应装配、KeyDictionaryFactory 的加密、MemberFactory/ClusterFactory 的成员与集群元数据、
CPUStatsFactory 的统计装配属于后续主题；不移植 Go 对象池或把这些全局工厂放入 core。
[context.go](../../pkg/core/context.go) 的 MetaProcessContext/ContextTODO 是 Go 宿主装配便利项；59 保留追踪契约，
全局 Runner 移植不在本计划范围。领域索引用显式取消/交错控制，不新建私有运行时。
[metrics.go](../../pkg/core/metrics.go) 的 init/MustRegister、锁等待、bucket 和树操作采集是宿主责任；
59 仅交付阶段追踪，其他指标由后续统计/运行时主题装配，68/69 审核不能误称已经兼容。

## 已有叶子能力与实际差距

| 依赖源码 | 已存在的 Rust API / 测试源码 | 集成缺口与责任 |
| --- | --- | --- |
| [constant](../../pkg/core/constant/kind.rs) | PriorityLevel、ResourceKind、SchedulePolicy、KeyType、Direction、ScheduleKind；kind_test.rs | 仅枚举能力，9/18/20 使用，非 Peer/Store |
| [rangetree](../../pkg/core/rangetree/lib.rs) | RangeItem、RangeTree::with_factory/update/find/get_overlaps/remove/scan_range/get_adjacent_item/get_at/get_with_index；range_tree_test.rs | Go region_tree.go 使用 btree 而非该通用树；39–43 必须另验引用计数、统计、随机候选与边界 |
| [btree](../../pkg/btree) | 已有 canonical crate 与排序索引实现 | 39–43 不得用通用树存在替代 RegionTree 统计/随机行为证据 |
| [movingaverage](../../pkg/movingaverage/hma.rs) | Hma::with_size/add/get/reset/set/get_instantaneous；hma_test.rs | Go NewHMA(60) 与 Rust 默认构造不同，15 显式选 60 并验证截断/负值；16 核对共享 |
| [ratelimit](../../pkg/ratelimit/rate_limiter.rs) | RateLimiter 的 async 查询与消费；rate_limiter_test.rs | 19 不能假定同步接口；StoreRateLimit 封装与共享仍需装配 |
| [Runner](../../pkg/ratelimit/runner.rs) | ConcurrentRunner/SyncRunner、取消、start/run_task/stop；runner_test.rs | 不等于 Go Runner trait，可用闭包和异步完成语义须在宿主核对；59 不承诺完整移植 |
| [storelimit](../../pkg/core/storelimit/lib.rs) | StoreRateLimit、Window、SlidingWindows、Type，任务 2–7 独立测试 | 尚无统一 Go StoreLimit 同形动态接口；19 负责 v1/v2 统一装配，16/27 保留共享/控制 |
| [proto](../../pkg/proto) | 固定 kvproto 来源、生成类型与 wire fixtures | 仅 60/61 依赖；core 禁止 proto、transport、具体 storage |

## 编号责任的行为验收含义

矩阵中的编号按下表解释；这些是验收要求，不是当前已提供能力。

| 编号 | 行为与用途 |
| --- | --- |
| 9 | 定义自有 Peer 与 PeerRole，迁移 voter、learner、incoming、demoting、witness 分类和 joint 计数，禁止引用 protobuf。 |
| 10 | 自有 RegionMeta、RegionEpoch、ID 和半开 KeyRange，空 end 表示无穷，保留 u64 边界。 |
| 11 | 自有 StoreMeta、地址、版本、部署路径、启动时间及 StoreInfo 默认权重和零值。 |
| 12 | Up、Preparing、Serving、Removing、Removed 与物理销毁标记的组合和转换。 |
| 13 | 标签合并、重复键、大小写与空值，以及 TiKV、TiFlash write、compute 和引擎校验错误。 |
| 14 | 自有 StoreStats 与 DFSStats 的容量、可用量、忙状态、快照计数和更新。 |
| 15 | GetAvgAvailable、climp0 与连续统计样本的平滑。 |
| 16 | Clone 与 ShallowClone 的元数据隔离、统计及 limiter 共享边界。 |
| 17 | DownTime、NeedPersist、IsDisconnected、IsUnhealthy、uptime 和 NeedAwakenStore 的阈值，显式输入当前时间。 |
| 18 | Pause 与 Resume 的入出方向、slow、stopping、slow-trend 驱逐及恢复。 |
| 19 | StoreInfo 的 v1-v2 limiter 装配、ResetStoreLimit、Feedback 与优先级可用性，使用真实 storelimit 实现。 |
| 20 | leader、region、learner、witness、pending 的计数与 size、weight setter 和 ResourceKind 投影。 |
| 21 | 高空间、过渡、低空间三个区间和权重、delta 边界，数值与 Go oracle 对齐。 |
| 22 | v2 公式与版本路由、零容量、低空间阈值和增减 delta。 |
| 23 | StorageSize、AvailableRatio、IsLowSpace、slow score、network slow score 和 min resolved TS 可用性。 |
| 24 | CompareLocation 与 DistinctScore 的首个差异层、缺失 label 和同位置得分。 |
| 25 | 真实拓扑权重，覆盖部分有效 label、空值、大小写、无合格 Store 及超过 16 层标签，不移植 Go 对象池。 |
| 26 | 新增、覆盖、删除、重置和列表快照，删除 Store 不应用旧 patch。 |
| 27 | 按 Store ID 暂停、恢复、驱逐、limit 重置及缺失 Store 错误。 |
| 28 | 自有 RegionInfo 构造、leader 与 follower、down 与 pending、learner 与 witness 的查询和排序。 |
| 29 | Clone 后键、peer、leader、epoch、flashback 修改不污染原快照。 |
| 30 | 领域 heartbeat 统计归一化、字节到 MiB、IAKV 与 columnar size 和流量舍入。 |
| 31 | 读写 query 合并与重置、GetLoads 与写入速率，零和反向 interval 按 Go 行为归一化。 |
| 32 | IsEmptyRegion、NeedMerge、IsOversized 的 size 和 keys 阈值，含空 Region 的特殊值。 |
| 33 | 来自 Storage、Sync、Heartbeat 的来源语义、流量与 bucket 继承条件，保留真实已观测行为。 |
| 34 | UpdateBuckets、CompareAndSetReportBuckets 与 bucketMeta 新旧版本接受规则，测试竞争更新只能留下有效版本。 |
| 35 | GenerateRegionGuideFunc 的保存元信息、保存缓存、同步和统计变更判定，覆盖逻辑存储量、leader、peer、term、flashback 与 buckets。 |
| 36 | Rule 序列化、键十六进制大写、DiffRegionPeersInfo 与 DiffRegionKeyInfo 和 HexMeta 的可观察字符串。 |
| 37 | SplitRegions 的键边界、epoch 和新 ID 生成行为，按 Go 输入输出核验，生产路径不硬编码测试 fixture。 |
| 38 | MergeRegions 的相邻键区间、peer、epoch 和边界结果，独立于 merge 资格判断。 |
| 39 | RegionItem 排序、半开区间 search、searchPrev、批量 key 查询及相邻项。 |
| 40 | 重叠替换、相同 ID 改范围、split 与 merge 和 remove 的区间不变量。 |
| 41 | updateRef、updateStat、size 与 write rate、来源计数在替换和删除后不重复累计。 |
| 42 | scanRange 提前停止、范围计数、空边界与有洞索引的扫描。 |
| 43 | 连续和不连续范围中的随机候选始终有效，空候选与重试边界明确。 |
| 44 | RegionsInfo 的 ID 表、root tree 和 SetRegion 替换结果一致，GetRegion 与 GetRegions 反映同一快照。 |
| 45 | PreCheckPutRegion、check 与 ValidRegion 对 epoch、term、重建 Region 及 overlap 的接受拒绝和错误语义。 |
| 46 | CheckAndPutRootTree、CheckAndPutRegion 与 AtomicCheckAndPutRegion 在校验和提交间不接受旧版本，返回正确 overlaps。 |
| 47 | leader、follower、learner、witness、pending 子树随 peer 和范围变化移除旧成员并加入新成员。 |
| 48 | CheckAndPutSubTree 和 UpdateSubTreeOrderInsensitive 重放 root 新旧事件仍收敛。 |
| 49 | RemoveRegion、RemoveRegionIfExist、ResetRegionCache 清理 root、ID、subtree 与逻辑 ref。 |
| 50 | QueryRegions 按 keys、prevKeys 和 IDs 返回对应结果，重复与缺失输入、128 分批边界正确。 |
| 51 | ScanRegions、ScanRegionWithIterator 的半开范围与 limit 为零、负数和跨洞行为。 |
| 52 | BatchScanRegions 的 WithLimit 与 OutputMustContainAllKeyRange，范围缺洞报错和多范围总限制。 |
| 53 | 各角色 count、size、write rate、平均 size、非 Storage 来源计数和 IsStorePrepared 的 0.9 门槛由同一组索引推导。 |
| 54 | GetRegionSizeByRange、角色范围 count、GetRangeHoles 与相邻 Region，含无界范围和部分相交。 |
| 55 | RandLeader、Follower、Learner、Witness、Pending 及 NeedTransferWitnessLeader 只返回合格 Region，角色变化后没有旧候选。 |
| 56 | GetFollowerStores、GetRegionStores、GetLeaderStore、GetNonWitnessVoterStores 对未知 Store 和 witness 返回准确结果。 |
| 57 | BasicCluster 组合已验证 StoresInfo 与 RegionsInfo，UpdateStoreStatus 和 UpdateAllStoreStatus 的计数、size 与 leader store 查找一致。 |
| 58 | GetStoresLeaderWriteRate 与 GetStoresWriteRate 的 Store ID、bytes、keys 三数组一一对应，空集和缺失 Store 正确。 |
| 59 | 在适配层以宿主 Registry 和显式时钟记录阶段耗时，每阶段独立且时钟回退不计负值。 |
| 60 | 在 server 转换自有 Store、StoreStats 与 kvproto，覆盖缺省字段、未知枚举和完整字段映射。 |
| 61 | 在 server 转换 Region、Peer、epoch 与 heartbeat stats，保留角色、bucket、replication、query 和 size 单位。 |

## 两个 failpoint 的实际时序

1. `concurrentBucketHeartbeat`：region.go:680，CompareAndSetReportBuckets 先读取 old 并判断版本；
   低/相等版本提前返回 true，不到注入点。较新版本到达注入点后睡眠 500ms，再做指针 CAS，CAS 失败由调用方重试。
   [cluster_test.go](../../server/cluster/cluster_test.go) 的 TestConcurrentReportBucket 先缓存 Region，再启用注入并发报告 v3，
   等待 100ms 后禁用注入、报告 v2，最后等待 v3 完成并断言最终 bucket 为 v3。
   禁用不撤销已开始的睡眠；启用也不会重放已完成的版本读取。34 验收版本/CAS 行为，61 转换 bucket，心跳服务重试装配属后续主题。
2. `UpdateSubTree`：region.go:1419，位于子树锁之前，仅 origin==nil 睡眠 1s；
   root 更新已发生，允许另一次心跳在其后更新 root/子树。origin!=nil 即使启用也不睡眠。
   [region_test.go](../../pkg/core/region_test.go) 的 TestSetRegionConcurrence 在创建 RegionsInfo 前启用，
   并发两次 AtomicCheckAndPutRegion，join 后禁用；不是启用时自动执行更新。
   46 固定 root 校验/写入，47/48 固定角色子树与乱序收敛；Rust 用确定性交错闸门证明旧更新不覆盖新更新，不能只移植 sleep。

两个注入点目前都没有 Rust 对应点；不能以运行时 enable/disable 或普通串行测试认定故障覆盖。
Go 验证必须走 make gotest 自动启停注入，禁止在注入开启时编辑/Cargo/git。

## 生产文件、符号与调用证据矩阵

以下每项函数/类型均按源码声明列出，含内部辅助；接口方法、常量和变量另列。
“引用”是 token 精确检索所得的代表位置，不是类型解析后的完整调用图，同名 getter 必须结合接收者审查。
“测试”是测试函数中出现该符号的静态证据，不代表独立边界覆盖或本次执行；无直接引用时明确待补。
根包所有行的 Rust 状态均为**缺失**，责任编号表示未来验收，而非已完成。内部辅助按拥有行为一起验证。
子包 storelimit 的完整声明见 [storelimit 契约](storelimit-contract.md)（其初始“缺失”状态为任务 1 的历史快照，以本文前序证据更新）；
constant/rangetree 已有 API 如上，具体源码/测试不纳入根包未实现计数。

### basic_cluster.go

源文件：[basic_cluster.go](../../pkg/core/basic_cluster.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `BasicCluster` (23) | 公开；类型/接口 | `server/server.go:187` | `pkg/mcs/scheduling/server/server_test.go:TestGetClusterBeforeStoredDoesNotPanic` | 26/27/50–58 |
| `NewBasicCluster` (29) | 公开；函数/方法 | `server/server.go:531` | `pkg/mcs/router/server/meta/watcher_test.go:TestNewWatcherWaitLoadFailed` | 26/27/50–58 |
| `UpdateStoreStatus` (37, `BasicCluster`) | 公开；函数/方法 | `pkg/core/store.go:1072` | `pkg/schedule/schedulers/scatter_range_test.go:TestScatterRangeBalance` | 26/27/50–58 |
| `GetLeaderStoreByRegionID` (45, `BasicCluster`) | 公开；函数/方法 | `server/grpc_service.go:1187` | `tests/integrations/mcs/scheduling/server_test.go:TestMultipleServerTestSuite` | 26/27/50–58 |
| `getWriteRate` (54, `BasicCluster`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 58 |
| `GetStoresLeaderWriteRate` (75, `BasicCluster`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 58 |
| `GetStoresWriteRate` (80, `BasicCluster`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:942` | 无直接用例，负责任务补 oracle | 58 |
| `UpdateAllStoreStatus` (85, `BasicCluster`) | 公开；函数/方法 | `tests/testutil.go:282` | `server/cluster/cluster_test.go:TestRemovingProcess` | 26/27/50–58 |
| `RegionSetInformer` (97) | 公开；类型/接口 | `pkg/schedule/core/cluster_informer.go:77` | 无直接用例，负责任务补 oracle | 26/27/50–58 |
| `batchScanRegionsOptions` (117) | 内部；类型/接口 | `pkg/core/region.go:2160` | 无直接用例，负责任务补 oracle | 52 |
| `BatchScanRegionsOptionFunc` (123) | 公开；类型/接口 | `pkg/core/region.go:2155` | 无直接用例，负责任务补 oracle | 52 |
| `WithLimit` (126) | 公开；函数/方法 | `client/opt/option.go:348` | `pkg/election/leadership_test.go:TestExitWatch` | 52 |
| `WithOutputMustContainAllKeyRange` (133) | 公开；函数/方法 | `client/opt/option.go:322` | `tests/integrations/client/client_test.go:TestClientWatchWithRevision` | 52 |
| `StoreSetInformer` (140) | 公开；类型/接口 | `pkg/statistics/store.go:138` | 无直接用例，负责任务补 oracle | 26/27/50–58 |
| `StoreSetController` (152) | 公开；类型/接口 | `pkg/schedule/core/cluster_informer.go:76` | 无直接用例，负责任务补 oracle | 26/27/50–58 |

### context.go

源文件：[context.go](../../pkg/core/context.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `MetaProcessContext` (24) | 公开；类型/接口 | `pkg/core/region.go:882` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `ContextTODO` (35) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestNeedSync` | 59；宿主/后续主题 |

### factory.go

源文件：[factory.go](../../pkg/core/factory.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |

### metrics.go

源文件：[metrics.go](../../pkg/core/metrics.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `init` (129) | 内部；函数/方法 | `pkg/core/store.go:584` | `pkg/mcs/resourcemanager/redirector/metadata_handler_test.go:TestRMMetadataFallbackHandlerReturnsInitError` | 59；宿主/后续主题 |
| `RegionHeartbeatProcessTracer` (146) | 公开；类型/接口 | `pkg/core/context.go:26` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `noopHeartbeatProcessTracer` (175) | 内部；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `NewNoopHeartbeatProcessTracer` (178) | 公开；函数/方法 | `pkg/core/context.go:38` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `Begin` (183, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/keyspace/metrics.go:81` | `tests/integrations/realcluster/ts_test.go:TestTS` | 59；宿主/后续主题 |
| `OnPreCheckFinished` (186, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1348` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnAsyncHotStatsFinished` (189, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1358` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnRegionGuideFinished` (192, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1363` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSaveCacheBegin` (195, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1396` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSaveCacheFinished` (198, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1407` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnCheckOverlapsFinished` (201, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1156` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnValidateRegionFinished` (204, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1160` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSetRegionFinished` (207, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1166` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnUpdateSubTreeFinished` (210, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1168` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnCollectRegionStatsFinished` (213, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1449` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnAllStageFinished` (216, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster_worker.go:65` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 59；宿主/后续主题 |
| `Release` (219, `noopHeartbeatProcessTracer`) | 公开；函数/方法 | `client/inner_client.go:125` | `pkg/ratelimit/concurrency_limiter_test.go:TestConcurrencyLimiter2` | 59；宿主/后续主题 |
| `regionHeartbeatProcessTracer` (225) | 内部；类型/接口 | 本文件内部/未见外部引用 | `pkg/core/metrics_test.go:TestRegionHeartbeatTracerToleratesNonMonotonicClock` | 59；宿主/后续主题 |
| `NewHeartbeatProcessTracer` (230) | 公开；函数/方法 | `server/cluster/cluster_worker.go:43` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 59；宿主/后续主题 |
| `Begin` (235, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/keyspace/metrics.go:81` | `tests/integrations/realcluster/ts_test.go:TestTS` | 59；宿主/后续主题 |
| `measure` (251, `regionHeartbeatProcessTracer`) | 内部；函数/方法 | `pkg/election/metrics.go:55` | `pkg/core/metrics_test.go:TestRegionHeartbeatTracerRecordsEachStageIndependently` | 59；宿主/后续主题 |
| `resetCheckpoint` (264, `regionHeartbeatProcessTracer`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnPreCheckFinished` (269, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1348` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnAsyncHotStatsFinished` (274, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1358` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnRegionGuideFinished` (279, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1363` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSaveCacheBegin` (284, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1396` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSaveCacheFinished` (289, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1407` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnCheckOverlapsFinished` (294, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1156` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnValidateRegionFinished` (299, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1160` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnSetRegionFinished` (304, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1166` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnUpdateSubTreeFinished` (309, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `pkg/core/region.go:1168` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnCollectRegionStatsFinished` (314, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster.go:1449` | 无直接用例，负责任务补 oracle | 59；宿主/后续主题 |
| `OnAllStageFinished` (319, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `server/cluster/cluster_worker.go:65` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 59；宿主/后续主题 |
| `Release` (325, `regionHeartbeatProcessTracer`) | 公开；函数/方法 | `client/inner_client.go:125` | `pkg/ratelimit/concurrency_limiter_test.go:TestConcurrencyLimiter2` | 59；宿主/后续主题 |

### peer.go

源文件：[peer.go](../../pkg/core/peer.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `IsLearner` (22) | 公开；函数/方法 | `pkg/core/region.go:161` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 9 |
| `IsWitness` (27) | 公开；函数/方法 | `pkg/core/region.go:166` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 9 |
| `IsVoter` (32) | 公开；函数/方法 | `pkg/schedule/checker/rule_checker.go:162` | 无直接用例，负责任务补 oracle | 9 |
| `IsVoterOrIncomingVoter` (38) | 公开；函数/方法 | `pkg/schedule/operator/step.go:906` | 无直接用例，负责任务补 oracle | 9 |
| `IsLearnerOrDemotingVoter` (48) | 公开；函数/方法 | `pkg/schedule/operator/step.go:914` | 无直接用例，负责任务补 oracle | 9 |
| `IsInJointState` (57) | 公开；函数/方法 | `pkg/schedule/operator/step.go:1066` | 无直接用例，负责任务补 oracle | 9 |
| `CountInJointState` (69) | 公开；函数/方法 | `pkg/schedule/operator/step.go:980` | 无直接用例，负责任务补 oracle | 9 |

### region.go

源文件：[region.go](../../pkg/core/region.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `errRegionIsStale` (60) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 45 |
| `RegionInfo` (67) | 公开；类型/接口 | `pkg/core/store.go:820` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestMain` | 28/49/53 |
| `RegionSource` (105) | 公开；类型/接口 | `pkg/core/region_option.go:424` | 无直接用例，负责任务补 oracle | 28/49/53 |
| `LoadedFromStorage` (117, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:180` | `tests/server/cluster/cluster_test.go:TestRegionStatistics` | 28/49/53 |
| `LoadedFromSync` (123, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `tests/server/cluster/cluster_test.go:TestRegionStatistics` | 28/49/53 |
| `IncRef` (128, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:168` | 无直接用例，负责任务补 oracle | 28/49/53 |
| `DecRef` (133, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:167` | 无直接用例，负责任务补 oracle | 28/49/53 |
| `GetRef` (138, `RegionInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1381` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 28/49/53 |
| `NewRegionInfo` (143) | 公开；函数/方法 | `tests/testutil.go:304` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRecordsSplitScatterInSchedulingService` | 28 |
| `classifyVoterAndLearner` (156) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28 |
| `peersEqualTo` (177, `RegionInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 29/35 |
| `rangeEqualsTo` (186, `RegionInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestCntRefAfterFlowOnlyUpdate` | 29/35 |
| `IsEmptyRegion` (209) | 公开；函数/方法 | `pkg/statistics/region.go:103` | `pkg/core/region_test.go:TestIsEmptyRegion` | 28/61 |
| `RegionHeartbeatResponse` (214) | 公开；类型/接口 | `pkg/core/factory.go:29` | `pkg/mcs/scheduling/server/server_test.go:TestClusterResourceGettersHandleNilReceiver` | 28/61 |
| `RegionHeartbeatRequest` (220) | 公开；类型/接口 | `tools/utils/meta.go:129` | `pkg/mcs/scheduling/server/server_test.go:TestRegionHeartbeatReturnsNotBootstrappedWhenHeartbeatStreamsMissing` | 28/61 |
| `RegionFromHeartbeat` (238) | 公开；函数/方法 | `server/grpc_service.go:1393` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30/61 |
| `Inherit` (295, `RegionInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1353` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 33 |
| `Clone` (323, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:121` | `pkg/mcs/resourcemanager/metadataapi/config_service_test.go:TestConfigServiceMetadataWriteDisabledReturns403` | 29 |
| `NeedMerge` (365, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region_collection.go:181` | `pkg/core/region_test.go:TestNeedMerge` | 32 |
| `IsOversized` (370, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region_collection.go:162` | 无直接用例，负责任务补 oracle | 32 |
| `GetTerm` (375, `RegionInfo`) | 公开；函数/方法 | `server/grpc_service.go:1488` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 28 |
| `GetLearners` (380, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/filter/healthy.go:57` | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 28 |
| `GetVoters` (385, `RegionInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:317` | `pkg/core/region_test.go:TestSortedEqual` | 28 |
| `GetWitnesses` (390, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/checker/rule_checker.go:628` | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 28 |
| `GetPeer` (395, `RegionInfo`) | 公开；函数/方法 | `server/handler.go:441` | `pkg/core/region_tree_test.go:TestRegionInfo` | 28 |
| `Rule` (405) | 公开；类型/接口 | `tests/testutil.go:463` | `pkg/mcs/scheduling/server/rule/watcher_test.go:TestRuleWatcherReconcilesInitialLoadRetry` | 36 |
| `String` (419, `Rule`) | 公开；函数/方法 | `pkg/core/store.go:952` | `pkg/mcs/scheduling/server/config/config_test.go:TestPersistConfigDefaultStoreLimit` | 36 |
| `NewRule` (433) | 公开；函数/方法 | `pkg/schedule/schedulers/init.go:594` | `pkg/core/region_test.go:TestCodecRule` | 36 |
| `MarshalJSON` (447, `Rule`) | 公开；函数/方法 | `client/http/types.go:435` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 36 |
| `UnmarshalJSON` (452, `Rule`) | 公开；函数/方法 | `client/http/types.go:461` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 36 |
| `GetDownPeer` (468, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/placement/fit.go:459` | `pkg/core/region_tree_test.go:TestRegionInfo` | 28 |
| `GetDownVoter` (478, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28 |
| `GetDownLearner` (488, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28 |
| `GetPendingPeer` (498, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/placement/fit.go:461` | `pkg/core/region_tree_test.go:TestRegionInfo` | 28 |
| `GetPendingVoter` (508, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/operator/step.go:230` | 无直接用例，负责任务补 oracle | 28 |
| `GetPendingLearner` (518, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/operator/step.go:534` | 无直接用例，负责任务补 oracle | 28 |
| `GetStorePeer` (528, `RegionInfo`) | 公开；函数/方法 | `server/handler.go:439` | `pkg/statistics/hot_peer_cache_test.go:TestCheckPeerFlowWithMoreExplicitPeersThanRegionPeers` | 28 |
| `GetStoreVoter` (538, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/operator/step.go:208` | 无直接用例，负责任务补 oracle | 28 |
| `GetStoreLearner` (548, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/operator/step.go:526` | 无直接用例，负责任务补 oracle | 28 |
| `GetStoreWitness` (558, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/filter/region_filters.go:172` | 无直接用例，负责任务补 oracle | 28 |
| `GetStoreIDs` (568, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:807` | `pkg/core/region_tree_test.go:TestRegionInfo` | 28 |
| `GetFollowers` (578, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:824` | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 28 |
| `GetNonWitnessVoters` (590, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:867` | 无直接用例，负责任务补 oracle | 28 |
| `GetDiffFollowers` (603, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28 |
| `GetID` (621, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:378` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 30/61 |
| `GetMeta` (626, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:235` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 30/61 |
| `GetStat` (634, `RegionInfo`) | 公开；函数/方法 | `pkg/syncer/server.go:453` | `tests/server/region_syncer/region_syncer_test.go:TestRegionSyncer` | 30/61 |
| `UpdateBuckets` (647, `RegionInfo`) | 公开；函数/方法 | `tests/testutil.go:719` | `pkg/core/region_test.go:TestInherit` | 34 |
| `GetReportBuckets` (662, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestInherit` | 34 |
| `CompareAndSetReportBuckets` (667, `RegionInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1332` | 无直接用例，负责任务补 oracle | 34 |
| `GetBuckets` (687, `RegionInfo`) | 公开；函数/方法 | `tests/testutil.go:719` | `pkg/core/region_test.go:TestInherit` | 34 |
| `GetStorePeerApproximateSize` (703, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region.go:146` | 无直接用例，负责任务补 oracle | 30 |
| `GetApproximateSize` (712, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:254` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 30 |
| `GetStorePeerApproximateKeys` (717, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region.go:147` | 无直接用例，负责任务补 oracle | 30 |
| `GetApproximateKvSize` (726, `RegionInfo`) | 公开；函数/方法 | `pkg/response/region.go:182` | `pkg/core/region_test.go:TestRegionFromHeartbeatIAKVSize` | 30 |
| `GetApproximateIAKvSize` (731, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/region.go:101` | `pkg/core/region_test.go:TestRegionFromHeartbeatIAKVSize` | 30 |
| `GetApproximateColumnarKvSize` (736, `RegionInfo`) | 公开；函数/方法 | `pkg/response/region.go:183` | 无直接用例，负责任务补 oracle | 30 |
| `GetApproximateKeys` (741, `RegionInfo`) | 公开；函数/方法 | `server/api/region.go:525` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 30 |
| `GetInterval` (746, `RegionInfo`) | 公开；函数/方法 | `server/handler.go:436` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `GetDownPeers` (751, `RegionInfo`) | 公开；函数/方法 | `pkg/response/region.go:173` | `pkg/core/region_test.go:TestSortedEqual` | 30 |
| `GetPendingPeers` (756, `RegionInfo`) | 公开；函数/方法 | `pkg/response/region.go:174` | `pkg/core/region_test.go:TestSortedEqual` | 30 |
| `GetCPUUsage` (763, `RegionInfo`) | 公开；函数/方法 | `server/api/region.go:744` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetReadCPUUsage` (768, `RegionInfo`) | 公开；函数/方法 | `pkg/schedule/scatter/region_scatterer.go:904` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetBytesRead` (776, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store_stats.go:99` | `pkg/statistics/hot_peer_cache_test.go:TestCacheInherit` | 30 |
| `GetRoundBytesRead` (781, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestRegionRoundingFlow` | 30 |
| `GetBytesWritten` (789, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store_stats.go:92` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetRoundBytesWritten` (794, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 30 |
| `GetKeysWritten` (802, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store_stats.go:106` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetKeysRead` (807, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store_stats.go:113` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 30 |
| `GetWriteRate` (812, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:177` | `pkg/core/region_test.go:TestRegionWriteRate` | 30 |
| `GetLeader` (822, `RegionInfo`) | 公开；函数/方法 | `pkg/core/store.go:849` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 28/10/30 |
| `GetStartKey` (827, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:36` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestCreateEmptyRegionTailUsesTiKVStore` | 28/10/30 |
| `GetFlowRoundDivisor` (832, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `tests/integrations/client/client_test.go:TestClientStatelessTestSuite` | 28/10/30 |
| `GetEndKey` (837, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:46` | `pkg/replication/replication_mode_test.go:TestRecoverProgressWithSplitAndMerge` | 28/10/30 |
| `GetPeers` (842, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_option.go:63` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestCreateEmptyRegionTailUsesTiKVStore` | 28/10/30 |
| `GetRegionEpoch` (847, `RegionInfo`) | 公开；函数/方法 | `pkg/core/region_option.go:144` | `pkg/storage/storage_test.go:TestTrySwitchRegionStorage` | 28/10/30 |
| `GetReplicationStatus` (852, `RegionInfo`) | 公开；函数/方法 | `server/server.go:869` | `pkg/replication/replication_mode_test.go:TestInitial` | 28/10/30 |
| `IsFlashbackChanged` (857, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 28/10/30 |
| `isInvolved` (861, `RegionInfo`) | 内部；函数/方法 | `pkg/core/region_tree.go:459` | 无直接用例，负责任务补 oracle | 10/45 |
| `isRegionRecreated` (865, `RegionInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 10/45 |
| `contain` (875, `RegionInfo`) | 内部；函数/方法 | `pkg/core/region_tree.go:322` | `pkg/mcs/discovery/register_test.go:TestRegister` | 10/45 |
| `RegionGuideFunc` (882) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 35 |
| `GenerateRegionGuideFunc` (886) | 公开；函数/方法 | `pkg/syncer/client.go:91` | `pkg/core/region_test.go:TestNeedSync` | 35 |
| `RWLockStats` (1030) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 59；宿主锁指标 |
| `Lock` (1039, `RWLockStats`) | 公开；函数/方法 | `pkg/core/store.go:910` | `pkg/syncer/server_test.go:TestSyncFullRegionsBuffersLiveRecords` | 59；宿主锁指标 |
| `Unlock` (1048, `RWLockStats`) | 公开；函数/方法 | `pkg/core/store.go:911` | `pkg/syncer/server_test.go:TestSyncFullRegionsBuffersLiveRecords` | 59；宿主锁指标 |
| `RLock` (1053, `RWLockStats`) | 公开；函数/方法 | `pkg/core/store.go:156` | `pkg/syncer/server_test.go:TestBindStreamWaitsForAppendHistoryRecordsBoundary` | 59；宿主锁指标 |
| `RUnlock` (1062, `RWLockStats`) | 公开；函数/方法 | `pkg/core/store.go:157` | `pkg/syncer/server_test.go:TestBindStreamWaitsForAppendHistoryRecordsBoundary` | 59；宿主锁指标 |
| `RegionsInfo` (1067) | 公开；类型/接口 | `pkg/core/basic_cluster.go:25` | `pkg/response/region_test.go:TestEasyJSONCompatibility` | 44 |
| `NewRegionsInfo` (1083) | 公开；函数/方法 | `pkg/core/basic_cluster.go:32` | `pkg/core/region_test.go:TestSetRegionConcurrence` | 44 |
| `GetRegion` (1098, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:46` | `pkg/syncer/server_test.go:TestClientFinishesFullSyncWithOldLeaderKeepAlive` | 44 |
| `getRegionLocked` (1105, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 44 |
| `CheckAndPutRegion` (1113, `RegionsInfo`) | 公开；函数/方法 | `pkg/syncer/client.go:205` | `pkg/storage/storage_test.go:TestLoadRegions` | 45/46 |
| `PutRegion` (1134, `RegionsInfo`) | 公开；函数/方法 | `pkg/syncer/client.go:162` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 45/46 |
| `PreCheckPutRegion` (1141, `RegionsInfo`) | 公开；函数/方法 | `pkg/syncer/client.go:150` | 无直接用例，负责任务补 oracle | 45/46 |
| `AtomicCheckAndPutRegion` (1148, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestSetRegionConcurrence` | 46 |
| `CheckAndPutRootTree` (1174, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1406` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 46 |
| `CheckAndPutSubTree` (1198, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1386` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 48 |
| `UpdateSubTreeOrderInsensitive` (1216, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestGetRegionSizeByRange` | 48 |
| `preUpdateSubTreeLocked` (1234, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 47/48 |
| `updateSubTreeLocked` (1275, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 47/48 |
| `getOverlapRegionFromOverlapTreeLocked` (1322, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 47/48 |
| `GetRelevantRegions` (1327, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 44 |
| `check` (1337) | 内部；函数/方法 | `pkg/core/store.go:577` | `pkg/mcs/utils/expected_primary_test.go:TestIsSamePrimary` | 45 |
| `SetRegion` (1361, `RegionsInfo`) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:72` | `pkg/core/region_test.go:TestSetRegion` | 44/46 |
| `setRegionLocked` (1367, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 44/46 |
| `UpdateSubTree` (1418, `RegionsInfo`) | 公开；函数/方法 | `pkg/ratelimit/runner.go:34` | `pkg/core/region_test.go:TestSetRegionConcurrence` | 47/48 |
| `updateSubTreeStat` (1434, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 47/48 |
| `TreeLen` (1460, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 44 |
| `GetOverlaps` (1467, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/region_tree.go:136` | `pkg/core/rangetree/range_tree_test.go:TestRingPutItem` | 44 |
| `RemoveRegion` (1474, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadPeerScheduleWithPlacementScope` | 49 |
| `ResetRegionCache` (1483, `RegionsInfo`) | 公开；函数/方法 | `server/server.go:1001` | `pkg/core/region_test.go:TestCntRefAfterResetRegionCache` | 49 |
| `RemoveRegionFromSubTree` (1500, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadPeerScheduleWithPlacementScope` | 49 |
| `removeRegionFromSubTreeLocked` (1508, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 49 |
| `RemoveRegionIfExist` (1522, `RegionsInfo`) | 公开；函数/方法 | `server/server.go:1040` | `server/cluster/cluster_test.go:TestRemovingProcess` | 49 |
| `peerSlice` (1529) | 内部；类型/接口 | `pkg/core/region_option.go:56` | 无直接用例，负责任务补 oracle | 28/35 |
| `Len` (1531, `peerSlice`) | 公开；函数/方法 | `pkg/core/region_tree.go:115` | `pkg/mcs/utils/expected_primary_test.go:TestDeleteExpectedPrimaryFlagRevokesLease` | 28/35 |
| `Swap` (1534, `peerSlice`) | 公开；函数/方法 | `pkg/keyspace/util.go:258` | `pkg/gc/gc_state_manager_test.go:TestGCStateManager` | 28/35 |
| `Less` (1537, `peerSlice`) | 公开；函数/方法 | `pkg/core/region_tree.go:51` | `pkg/mcs/discovery/register_test.go:TestRegisterRejectsUnleasedExistingKey` | 28/35 |
| `SortedPeersEqual` (1542) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestSortedEqual` | 28/35 |
| `peerStatsSlice` (1555) | 内部；类型/接口 | `pkg/core/region_option.go:35` | 无直接用例，负责任务补 oracle | 28/35 |
| `Len` (1557, `peerStatsSlice`) | 公开；函数/方法 | `pkg/core/region_tree.go:115` | `pkg/mcs/utils/expected_primary_test.go:TestDeleteExpectedPrimaryFlagRevokesLease` | 28/35 |
| `Swap` (1560, `peerStatsSlice`) | 公开；函数/方法 | `pkg/keyspace/util.go:258` | `pkg/gc/gc_state_manager_test.go:TestGCStateManager` | 28/35 |
| `Less` (1563, `peerStatsSlice`) | 公开；函数/方法 | `pkg/core/region_tree.go:51` | `pkg/mcs/discovery/register_test.go:TestRegisterRejectsUnleasedExistingKey` | 28/35 |
| `SortedPeersStatsEqual` (1568) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestSortedEqual` | 28/35 |
| `GetRegionByKey` (1583, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:109` | `tools/pd-simulator/simulator/simutil/key_test.go:TestRegionSplitKey` | 50 |
| `GetPrevRegionByKey` (1590, `RegionsInfo`) | 公开；函数/方法 | `pkg/utils/grpcutil/cluster.go:69` | 无直接用例，负责任务补 oracle | 50 |
| `GetRegions` (1597, `RegionsInfo`) | 公开；函数/方法 | `tests/cluster.go:422` | `pkg/syncer/server_test.go:TestSyncHistoryRecordsSplitBatches` | 50 |
| `GetStoreRegions` (1608, `RegionsInfo`) | 公开；函数/方法 | `tests/cluster.go:451` | `tools/pd-ctl/tests/region/region_test.go:TestRegionTestSuite` | 50 |
| `QueryRegions` (1627, `RegionsInfo`) | 公开；函数/方法 | `pkg/utils/grpcutil/cluster.go:210` | `pkg/core/region_test.go:TestQueryRegions` | 50 |
| `getRegionsByKeys` (1695, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 50 |
| `getRegionsByPrevKeys` (1708, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 50 |
| `getRegionsByIDs` (1720, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 50 |
| `sortOutKeyIDMap` (1734) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 50 |
| `SubTreeRegionType` (1765) | 公开；类型/接口 | `server/api/region.go:268` | `tools/pd-ctl/tests/region/region_test.go:TestRegionTestSuite` | 56 |
| `GetStoreRegionsByTypeInSubTree` (1783, `RegionsInfo`) | 公开；函数/方法 | `server/api/region.go:268` | `tools/pd-ctl/tests/region/region_test.go:TestRegionTestSuite` | 56 |
| `GetStoreLeaderRegionSize` (1819, `RegionsInfo`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:767` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/58 |
| `GetStoreFollowerRegionSize` (1826, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/58 |
| `GetStoreLearnerRegionSize` (1833, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/58 |
| `GetStoreRegionSize` (1840, `RegionsInfo`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:768` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/58 |
| `getStoreRegionSizeLocked` (1847, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 53/58 |
| `GetStoreLeaderWriteRate` (1852, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:76` | 无直接用例，负责任务补 oracle | 53/58 |
| `GetStoreWriteRate` (1859, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:81` | 无直接用例，负责任务补 oracle | 53/58 |
| `GetNotFromStorageRegionsCnt` (1875, `RegionsInfo`) | 公开；函数/方法 | `pkg/schedule/prepare_checker.go:53` | `server/cluster/cluster_test.go:TestShouldRun` | 53 |
| `GetNotFromStorageRegionsCntByStore` (1882, `RegionsInfo`) | 公开；函数/方法 | `pkg/schedule/prepare_checker.go:68` | 无直接用例，负责任务补 oracle | 53 |
| `IsStorePrepared` (1890, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:2027` | 无直接用例，负责任务补 oracle | 53 |
| `getNotFromStorageRegionsCntByStoreLocked` (1895, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 53 |
| `GetMetaRegions` (1900, `RegionsInfo`) | 公开；函数/方法 | `server/server.go:1761` | `pkg/storage/storage_test.go:TestLoadRegions` | 53/56 |
| `GetStoreStats` (1911, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/store.go:580` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 53/56 |
| `GetTotalRegionCount` (1919, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:98` | `pkg/storage/storage_test.go:TestLoadRegions` | 53/56 |
| `GetStoreRegionCount` (1926, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:105` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/56 |
| `getStoreRegionCountLocked` (1933, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 53/56 |
| `GetStorePendingPeerCount` (1938, `RegionsInfo`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:766` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/56 |
| `GetStoreLeaderCount` (1945, `RegionsInfo`) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:764` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 53/56 |
| `GetStoreFollowerCount` (1952, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/cluster/cluster_test.go:TestUpdateAndDeleteLabel` | 53/56 |
| `GetStoreLearnerCount` (1959, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 53/56 |
| `GetStoreWitnessCount` (1966, `RegionsInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/cluster/cluster_test.go:TestUpdateAndDeleteLabel` | 53/56 |
| `RandPendingRegions` (1973, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:103` | 无直接用例，负责任务补 oracle | 55 |
| `randLeaderRegion` (1980, `RegionsInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 55 |
| `RandLeaderRegions` (1987, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:100` | `pkg/core/region_test.go:TestShouldRemoveFromSubTree` | 55 |
| `RandFollowerRegions` (1994, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:99` | 无直接用例，负责任务补 oracle | 55 |
| `RandLearnerRegions` (2001, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:101` | 无直接用例，负责任务补 oracle | 55 |
| `RandWitnessRegions` (2008, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:102` | 无直接用例，负责任务补 oracle | 55 |
| `GetLeader` (2015, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/store.go:849` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 56 |
| `GetFollower` (2025, `RegionsInfo`) | 公开；函数/方法 | `tests/cluster.go:872` | `tests/integrations/mcs/resourcemanager/resource_manager_test.go:TestSwitchModeDuringWorkload` | 56 |
| `GetReadQueryNum` (2035, `RegionInfo`) | 公开；函数/方法 | `server/api/region.go:669` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetWriteQueryNum` (2040, `RegionInfo`) | 公开；函数/方法 | `server/api/region.go:641` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetReadQueryNum` (2045) | 公开；函数/方法 | `server/api/region.go:669` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetWriteQueryNum` (2053) | 公开；函数/方法 | `server/api/region.go:641` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetLoads` (2062, `RegionInfo`) | 公开；函数/方法 | `pkg/statistics/hot_peer.go:138` | `pkg/statistics/hot_peer_cache_test.go:TestCheckPeerFlowWithMoreExplicitPeersThanRegionPeers` | 31 |
| `GetWriteLoads` (2076, `RegionInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 31 |
| `GetStoreLeaderCountByRange` (2090, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:112` | `pkg/core/region_tree_test.go:TestStoreRegionCount` | 54 |
| `GetStorePeerCountByRange` (2100, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:114` | `pkg/core/region_tree_test.go:TestStoreRegionCount` | 54 |
| `GetStoreLearnerCountByRange` (2117, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:113` | `pkg/core/region_tree_test.go:TestStoreRegionCount` | 54 |
| `GetRegionCount` (2127, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/store.go:388` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 54 |
| `ScanRegions` (2135, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:108` | `pkg/utils/grpcutil/cluster_test.go:TestFollowerForwardAndHandleTestSuite` | 51 |
| `BatchScanRegions` (2155, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:110` | `tests/integrations/mcs/router/server_test.go:TestServerTestSuite` | 52 |
| `scanRegion` (2185) | 内部；函数/方法 | `tools/pd-ctl/pdctl/command/region_command.go:140` | `pkg/core/region_test.go:TestScanRegion` | 52 |
| `ScanRegionWithIterator` (2243, `RegionsInfo`) | 公开；函数/方法 | `tools/pd-simulator/simulator/cases/import_data.go:102` | 无直接用例，负责任务补 oracle | 54 |
| `GetRegionSizeByRange` (2250, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:2071` | `pkg/core/region_test.go:TestGetRegionSizeByRange` | 54 |
| `CollectWaitLockMetrics` (2288, `RegionsInfo`) | 公开；函数/方法 | `pkg/mcs/scheduling/server/cluster.go:801` | 无直接用例，负责任务补 oracle | 59；后续锁指标 |
| `GetAdjacentRegions` (2321, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:107` | `tools/pd-ctl/tests/region/region_test.go:TestRegionTestSuite` | 39/50 |
| `GetRangeHoles` (2336, `RegionsInfo`) | 公开；函数/方法 | `tests/cluster.go:444` | 无直接用例，负责任务补 oracle | 54/53 |
| `GetAverageRegionSize` (2362, `RegionsInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:104` | `pkg/schedule/schedulers/balance_region_test.go:TestSingleRegionOnLargeEmptyDiskDoesNotMigrate` | 54/53 |
| `ValidRegion` (2372, `RegionsInfo`) | 公开；函数/方法 | `server/cluster/cluster_worker.go:86` | `tests/server/cluster/cluster_work_test.go:TestRaftClusterTestSuite` | 45 |
| `DiffRegionPeersInfo` (2389) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_tree_test.go:TestRegionInfo` | 36 |
| `DiffRegionKeyInfo` (2419) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestRegionKey` | 36 |
| `ToUpperASCIIInplace` (2436) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 36 |
| `EncodeToString` (2458) | 公开；函数/方法 | `pkg/keyspace/util.go:153` | `pkg/keyspace/util_test.go:TestMakeLabelRule` | 36 |
| `HexRegionKey` (2466) | 公开；函数/方法 | `pkg/core/region_tree.go:439` | `server/api/region_test.go:TestRegionsInfoMarshal` | 36 |
| `HexRegionKeyStr` (2472) | 公开；函数/方法 | `pkg/response/region.go:168` | `pkg/storage/hot_region_storage_test.go:TestHotRegionWrite` | 36 |
| `RegionToHexMeta` (2478) | 公开；函数/方法 | `pkg/core/region_tree.go:201` | `pkg/core/region_test.go:TestRegionKey` | 36 |
| `HexRegionMeta` (2486) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 36 |
| `String` (2490, `HexRegionMeta`) | 公开；函数/方法 | `pkg/core/store.go:952` | `pkg/mcs/scheduling/server/config/config_test.go:TestPersistConfigDefaultStoreLimit` | 36 |
| `RegionsToHexMeta` (2499) | 公开；函数/方法 | `server/cluster/cluster_worker.go:259` | 无直接用例，负责任务补 oracle | 36 |
| `HexRegionsMeta` (2507) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 36 |
| `String` (2509, `HexRegionsMeta`) | 公开；函数/方法 | `pkg/core/store.go:952` | `pkg/mcs/scheduling/server/config/config_test.go:TestPersistConfigDefaultStoreLimit` | 36 |
| `NeedTransferWitnessLeader` (2521) | 公开；函数/方法 | `pkg/schedule/schedulers/scheduler_controller.go:493` | 无直接用例，负责任务补 oracle | 28/32 |
| `SplitRegions` (2529) | 公开；函数/方法 | `client/client.go:92` | `pkg/core/region_tree_test.go:TestRegionTreeSplitAndMerge` | 37 |
| `MergeRegions` (2554) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_tree_test.go:TestRegionTreeSplitAndMerge` | 38 |
| `NewTestRegionInfo` (2581) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/utils/grpcutil/cluster_test.go:TestFollowerForwardAndHandleTestSuite` | 28；测试辅助 |

### region_option.go

源文件：[region_option.go](../../pkg/core/region_option.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `RegionCreateOption` (29) | 公开；类型/接口 | `pkg/core/region.go:143` | `pkg/core/region_test.go:TestNeedSync` | 29 |
| `WithDownPeers` (32) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:141` | `pkg/statistics/region_collection_test.go:TestRegionStatistics` | 29 |
| `WithFlowRoundByDigit` (40) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/region_test.go:TestRegionRoundingFlow` | 30 |
| `GetFlowRoundDivisorByDigit` (48) | 公开；函数/方法 | `server/grpc_service.go:1284` | `tests/server/cluster/cluster_test.go:TestStaleTermHeartbeat` | 30 |
| `WithPendingPeers` (53) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:140` | `pkg/statistics/region_collection_test.go:TestRegionStatistics` | 29 |
| `WithWitness` (61) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:441` | `pkg/schedule/scatter/region_scatterer_test.go:TestScatterReservesPeerRole` | 29 |
| `WithWitnesses` (72) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/schedule/checker/rule_checker_test.go:TestRuleCheckerTestSuite` | 29 |
| `WithLearners` (87) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/schedule/checker/rule_checker_test.go:TestRuleCheckerTestSuite` | 29 |
| `WithLeader` (102) | 公开；函数/方法 | `pkg/schedule/operator/test_util.go:30` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 29 |
| `WithStartKey` (109) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:473` | `pkg/replication/replication_mode_test.go:TestStateSwitch` | 29 |
| `WithEndKey` (116) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:474` | `pkg/replication/replication_mode_test.go:TestStateSwitch` | 29 |
| `WithNewRegionID` (123) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:135` | `pkg/core/region_test.go:TestSetRegion` | 29 |
| `WithNewPeerIDs` (130) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:136` | 无直接用例，负责任务补 oracle | 29 |
| `WithIncVersion` (142) | 公开；函数/方法 | `tools/pd-simulator/simulator/raft.go:137` | `tests/integrations/mcs/router/server_test.go:TestServerTestSuite` | 29 |
| `WithDecVersion` (157) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 29 |
| `WithIncConfVer` (167) | 公开；函数/方法 | `tools/pd-simulator/simulator/task.go:311` | `pkg/schedule/placement/region_rule_cache_test.go:TestRegionRuleFitCache` | 29 |
| `WithDecConfVer` (182) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 29 |
| `WithFlashback` (192) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 29 |
| `SetCPUUsage` (200) | 公开；函数/方法 | 本文件内部/未见外部引用 | `server/api/region_test.go:TestRegionsInfoMarshal` | 30 |
| `SetWrittenBytes` (207) | 公开；函数/方法 | `pkg/syncer/client.go:140` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `SetWrittenKeys` (214) | 公开；函数/方法 | `pkg/syncer/client.go:141` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `WithRemoveStorePeer` (221) | 公开；函数/方法 | `pkg/schedule/operator/test_util.go:47` | `pkg/statistics/region_collection_test.go:TestRegionStatistics` | 29 |
| `SetBuckets` (234) | 公开；函数/方法 | `tests/testutil.go:718` | `pkg/core/region_test.go:TestInherit` | 29 |
| `SetReadBytes` (250) | 公开；函数/方法 | `pkg/syncer/client.go:142` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `SetReadKeys` (257) | 公开；函数/方法 | `pkg/syncer/client.go:143` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `SetReadQuery` (264) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:489` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 31 |
| `SetWrittenQuery` (272) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:565` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 31 |
| `SetQueryStats` (282) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |
| `AddQueryStats` (291) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |
| `SetApproximateSize` (299) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:433` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRecordsSplitScatterInSchedulingService` | 30 |
| `SetApproximateKvSize` (306) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/statistics/region_collection_test.go:TestRegionStatsObserveIAKVSize` | 30 |
| `SetApproximateIAKvSize` (313) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/statistics/region_collection_test.go:TestRegionStatsObserveIAKVSize` | 30 |
| `SetApproximateKeys` (320) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:433` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRecordsSplitScatterInSchedulingService` | 30 |
| `SetReportInterval` (328) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:488` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 30 |
| `SetRegionConfVer` (335) | 公开；函数/方法 | `server/cluster/cluster.go:1400` | `tests/integrations/mcs/scheduling/api_test.go:TestAPI` | 29 |
| `SetRegionVersion` (346) | 公开；函数/方法 | `server/cluster/cluster.go:1400` | `tests/integrations/mcs/scheduling/api_test.go:TestAPI` | 29 |
| `SetPeers` (357) | 公开；函数/方法 | `pkg/schedule/operator/builder.go:332` | `pkg/core/region_tree_test.go:TestRegionInfo` | 29 |
| `SetReplicationStatus` (364) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/replication/replication_mode_test.go:TestStateSwitch` | 29 |
| `WithAddPeer` (371) | 公开；函数/方法 | `pkg/schedule/operator/test_util.go:39` | `pkg/statistics/hot_peer_cache_test.go:TestFindOldHotPeerStatForInheritance` | 29 |
| `WithRole` (383) | 公开；函数/方法 | `tools/pd-simulator/simulator/task.go:311` | `pkg/schedule/operator/operator_controller_test.go:TestOperatorControllerTestSuite` | 29 |
| `WithReplacePeerStore` (394) | 公开；函数/方法 | `pkg/schedule/checker/affinity_checker.go:480` | 无直接用例，负责任务补 oracle | 29 |
| `WithReplaceLeaderStore` (405) | 公开；函数/方法 | `pkg/schedule/checker/affinity_checker.go:482` | 无直接用例，负责任务补 oracle | 29 |
| `WithInterval` (417) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/statistics/hot_peer_cache_test.go:TestHotPeerCacheTopNThreshold` | 30 |
| `SetSource` (424) | 公开；函数/方法 | `tests/testutil.go:303` | `server/server_test.go:TestDeleteFollowerRegion` | 29 |
| `RandomKindReadQuery` (431) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:737` | 无直接用例，负责任务补 oracle | 31 |
| `RandomKindWriteQuery` (451) | 公开；函数/方法 | `pkg/mock/mockcluster/mockcluster.go:745` | 无直接用例，负责任务补 oracle | 31 |
| `mergeQueryStat` (486) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |
| `resetReadQuery` (510) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |
| `resetWriteQuery` (519) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 31 |

### region_tree.go

源文件：[region_tree.go](../../pkg/core/region_tree.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `regionItem` (32) | 内部；类型/接口 | `pkg/core/region.go:1070` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestMain` | 39 |
| `GetStartKey` (37, `regionItem`) | 公开；函数/方法 | `pkg/core/region.go:187` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestCreateEmptyRegionTailUsesTiKVStore` | 39 |
| `GetID` (42, `regionItem`) | 公开；函数/方法 | `pkg/core/store.go:378` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 39 |
| `GetEndKey` (47, `regionItem`) | 公开；函数/方法 | `pkg/core/region.go:187` | `pkg/replication/replication_mode_test.go:TestRecoverProgressWithSplitAndMerge` | 39 |
| `Less` (52, `regionItem`) | 公开；函数/方法 | `pkg/core/region.go:1537` | `pkg/mcs/discovery/register_test.go:TestRegisterRejectsUnleasedExistingKey` | 39 |
| `regionTree` (62) | 内部；类型/接口 | `pkg/core/region.go:1069` | `pkg/core/region_tree_test.go:TestRegionTree` | 39 |
| `newRegionTree` (74) | 内部；函数/方法 | `pkg/core/region.go:1298` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestCreateEmptyRegionDisableParanoidCheck` | 39 |
| `newRegionTreeWithCountRef` (84) | 内部；函数/方法 | `pkg/core/region.go:1085` | 无直接用例，负责任务补 oracle | 39 |
| `GetCountByRange` (96, `regionTree`) | 公开；函数/方法 | `pkg/core/region.go:2094` | 无直接用例，负责任务补 oracle | 42 |
| `length` (122, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1459` | `pkg/tso/keyspace_group_manager_test.go:TestFinishSplitKeyspaceGroupDoesNotBlockStateReads` | 41 |
| `notFromStorageRegionsCount` (129, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1878` | 无直接用例，负责任务补 oracle | 41 |
| `overlaps` (137, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1078` | `pkg/core/rangetree/range_tree_test.go:TestRingPutItem` | 40 |
| `updateRef` (163, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1267` | 无直接用例，负责任务补 oracle | 41 |
| `update` (174, `regionTree`) | 内部；函数/方法 | `pkg/core/store.go:161` | `pkg/mcs/tso/server/config_test.go:TestLoadFromConfig` | 40 |
| `updateStat` (219, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1396` | 无直接用例，负责任务补 oracle | 41 |
| `remove` (244, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:94` | `pkg/mcs/utils/expected_primary_test.go:TestMain` | 40 |
| `search` (268, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:53` | `pkg/core/region_tree_test.go:TestRegionTree` | 39/50 |
| `searchPrev` (278, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1593` | `pkg/core/region_tree_test.go:TestRegionTree` | 39/50 |
| `searchByKeys` (295, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1701` | 无直接用例，负责任务补 oracle | 39/50 |
| `searchByPrevKeys` (305, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1712` | 无直接用例，负责任务补 oracle | 39/50 |
| `find` (315, `regionTree`) | 内部；函数/方法 | `pkg/core/store.go:584` | `pkg/btree/btree_generic_test.go:TestBTreeG` | 39/50 |
| `scanRange` (331, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:2139` | 无直接用例，负责任务补 oracle | 42 |
| `scanRanges` (348, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1613` | `pkg/core/region_tree_test.go:TestStoreRegionCount` | 42 |
| `getAdjacentRegions` (360, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:2324` | `pkg/core/region_tree_test.go:TestRegionTree` | 39 |
| `getAdjacentItem` (366, `regionTree`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 39 |
| `randomRegion` (384, `regionTree`) | 内部；函数/方法 | `pkg/core/region.go:1983` | `pkg/core/region_tree_test.go:TestRandomRegion` | 43 |
| `RandomRegions` (393, `regionTree`) | 公开；函数/方法 | `pkg/core/region.go:1976` | `pkg/core/region_tree_test.go:TestRandomRegion` | 43 |
| `TotalSize` (499, `regionTree`) | 公开；函数/方法 | `pkg/core/region.go:1822` | `pkg/core/region_test.go:TestSetRegion` | 41 |
| `TotalWriteRate` (508, `regionTree`) | 公开；函数/方法 | `pkg/core/region.go:1855` | `pkg/core/region_test.go:TestSetRegion` | 41 |

### store.go

源文件：[store.go](../../pkg/core/store.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `StoreInfo` (63) | 公开；类型/接口 | `pkg/core/store_option.go:29` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestEpochComparison` | 11 |
| `NewStoreInfo` (88) | 公开；函数/方法 | `server/cluster/cluster.go:1639` | `pkg/mcs/scheduling/server/server_test.go:TestRegionHeartbeatReturnsNotBootstrappedWhenHeartbeatStreamsMissing` | 11 |
| `NewStoreInfoWithLabel` (104) | 公开；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/store_topology_test.go:TestTopologyWeight` | 11 |
| `Clone` (122, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:322` | `pkg/mcs/resourcemanager/metadataapi/config_service_test.go:TestConfigServiceMetadataWriteDisabledReturns403` | 16 |
| `LimitVersion` (155, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 16 |
| `Feedback` (162, `StoreInfo`) | 公开；函数/方法 | `pkg/core/storelimit/limit.go:60` | `pkg/core/storelimit/rust_core_task_03_oracle_test.go:TestRustCoreTask03Oracle` | 16 |
| `ShallowClone` (169, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 16 |
| `AllowLeaderTransferIn` (201, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:128` | `server/cluster/cluster_test.go:TestStores` | 18/19 |
| `AllowLeaderTransferOut` (207, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:129` | `pkg/schedule/schedulers/grant_leader_test.go:TestGrantLeaderScheduler` | 18/19 |
| `EvictedAsSlowStore` (212, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/filter/filters.go:400` | `pkg/schedule/schedulers/evict_slow_store_test.go:TestEvictSlowStoreTestSuite` | 18/19 |
| `EvictedAsStoppingStore` (217, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/filter/filters.go:409` | 无直接用例，负责任务补 oracle | 18/19 |
| `IsEvictedAsSlowTrend` (222, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/filter/filters.go:418` | 无直接用例，负责任务补 oracle | 18/19 |
| `IsAvailable` (227, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/filter/filters.go:445` | `server/cluster/cluster_test.go:TestCheckCache` | 18/19 |
| `IsTiKV` (234, `StoreInfo`) | 公开；函数/方法 | `server/server.go:1395` | `pkg/statistics/store_collection_test.go:TestTiKVNotAffectedByTiFlashCompute` | 13 |
| `IsTiFlashWrite` (240, `StoreInfo`) | 公开；函数/方法 | `pkg/statistics/collector.go:101` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestSelectLeader` | 13 |
| `IsTiFlashCompute` (245, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/handler/handler.go:1022` | `pkg/core/store_test.go:TestStoreInfoIsTiFlash` | 13 |
| `Engine` (250, `StoreInfo`) | 公开；函数/方法 | `server/api/stats.go:46` | `tests/integrations/mcs/scheduling/config_test.go:TestConfig` | 13 |
| `IsUp` (258, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1805` | `server/cluster/cluster_test.go:TestSetOfflineStore` | 12 |
| `IsPreparing` (263, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_topology.go:73` | 无直接用例，负责任务补 oracle | 12 |
| `IsServing` (268, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_topology.go:73` | `pkg/mcs/resourcemanager/server/manager_test.go:TestMain` | 12 |
| `IsRemoving` (273, `StoreInfo`) | 公开；函数/方法 | `pkg/progress/progress.go:169` | `pkg/schedule/checker/affinity_checker_test.go:TestAffinityAvailabilityCheckWithOfflineStore` | 12 |
| `IsRemoved` (278, `StoreInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:62` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestRemoveFailedStores` | 12 |
| `GetSlowScore` (283, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:121` | 无直接用例，负责任务补 oracle | 23 |
| `GetNetworkSlowScores` (290, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:122` | 无直接用例，负责任务补 oracle | 23 |
| `GetNetworkSlowTriggers` (300, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/schedulers/evict_slow_store.go:552` | 无直接用例，负责任务补 oracle | 23 |
| `WitnessScore` (308, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/placement/fit.go:140` | 无直接用例，负责任务补 oracle | 23 |
| `IsSlow` (313, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:446` | 无直接用例，负责任务补 oracle | 23 |
| `IsStopping` (320, `StoreInfo`) | 公开；函数/方法 | `pkg/schedule/schedulers/evict_stopping_store.go:289` | `pkg/schedule/schedulers/evict_stopping_store_test.go:TestEvictStoppingStoreTestSuite` | 23 |
| `GetSlowTrend` (327, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:98` | 无直接用例，负责任务补 oracle | 23 |
| `IsPhysicallyDestroyed` (334, `StoreInfo`) | 公开；函数/方法 | `pkg/statistics/store.go:143` | `server/cluster/cluster_test.go:TestSetOfflineStore` | 23 |
| `DownTime` (339, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:146` | 无直接用例，负责任务补 oracle | 17 |
| `GetMeta` (344, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:625` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 11/12/13 |
| `GetState` (349, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:1011` | `pkg/keyspace/tso_keyspace_group_test.go:TestRemoveKeyspacesFromGroupStaysWithinEtcdLimits` | 11/12/13 |
| `GetNodeState` (354, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:334` | `tests/integrations/client/client_test.go:TestClientStatelessTestSuite` | 11/12/13 |
| `GetStatusAddress` (359, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:328` | 无直接用例，负责任务补 oracle | 11/12/13 |
| `GetAddress` (364, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:327` | `tests/server/cluster/cluster_test.go:TestGetPutConfig` | 11/12/13 |
| `GetVersion` (369, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:655` | `pkg/storage/storage_test.go:TestTrySwitchRegionStorage` | 11/12/13 |
| `GetLabels` (374, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:333` | `pkg/core/store_topology_test.go:TestTopologyWeightWithEmptyStoreLabelValue` | 11/12/13 |
| `GetID` (379, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:620` | `pkg/mcs/scheduling/server/grpc_service_test.go:TestAskBatchSplitRejectsAffinityAutoSplit` | 11/12/13 |
| `GetLeaderCount` (384, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:111` | `tests/server/cluster/cluster_test.go:TestTransferLeaderForScheduler` | 20 |
| `GetRegionCount` (389, `StoreInfo`) | 公开；函数/方法 | `pkg/core/region.go:2126` | `pkg/core/region_test.go:TestUpdateRegionEquivalence` | 20 |
| `GetLearnerCount` (394, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:119` | 无直接用例，负责任务补 oracle | 20 |
| `GetWitnessCount` (399, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:120` | 无直接用例，负责任务补 oracle | 20 |
| `GetLeaderSize` (404, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:114` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 20 |
| `GetRegionSize` (409, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:118` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 20 |
| `GetPendingPeerCount` (414, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:126` | `server/cluster/cluster_test.go:TestUpdateAndDeleteLabel` | 20 |
| `GetLeaderWeight` (419, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:112` | `pkg/storage/storage_test.go:TestStoreWeight` | 20 |
| `GetRegionWeight` (424, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:116` | `pkg/storage/storage_test.go:TestStoreWeight` | 20 |
| `GetLastHeartbeatTS` (429, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:137` | `pkg/statistics/store_collection_test.go:TestStoreStatistics` | 17/19 |
| `NeedPersist` (434, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1232` | 无直接用例，负责任务补 oracle | 17/19 |
| `GetStoreLimit` (439, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1199` | `pkg/mcs/scheduling/server/config/config_test.go:TestPersistConfigDefaultStoreLimit` | 17/19 |
| `LeaderScore` (449, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:60` | `pkg/schedule/schedulers/balance_leader_test.go:TestBalanceLeaderMaxBatchSchedule` | 20 |
| `RegionScore` (464, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:64` | `pkg/core/store_test.go:TestRegionScore` | 20 |
| `regionScoreV1` (475, `StoreInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 21 |
| `regionScoreV2` (520, `StoreInfo`) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/store_test.go:TestLowSpaceScoreV2` | 22 |
| `StorageSize` (565, `StoreInfo`) | 公开；函数/方法 | `client/http/types.go:321` | `tests/server/api/stats_test.go:TestStatTestSuite` | 23/20 |
| `AvailableRatio` (570, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 23/20 |
| `IsLowSpace` (579, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1226` | `pkg/core/store_test.go:TestLowSpaceRatio` | 23/20 |
| `ResourceCount` (593, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 23/20 |
| `ResourceSize` (605, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 23/20 |
| `ResourceWeight` (617, `StoreInfo`) | 公开；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 23/20 |
| `GetStartTime` (637, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:134` | `pkg/schedule/operator/operator_test.go:TestPeerInfluenceUsesRegionKeys` | 17 |
| `GetUptime` (642, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:140` | 无直接用例，负责任务补 oracle | 17 |
| `GetMinResolvedTS` (651, `StoreInfo`) | 公开；函数/方法 | `server/api/router.go:369` | `tests/server/cluster/cluster_test.go:TestTransferLeaderForScheduler` | 17 |
| `NeedAwakenStore` (657, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1888` | `server/cluster/cluster_test.go:TestAwakenStore` | 17 |
| `IsDisconnected` (672, `StoreInfo`) | 公开；函数/方法 | `pkg/response/store.go:148` | `pkg/schedule/scatter/region_scatterer_test.go:TestSomeStoresFilteredScatterGroupInConcurrency` | 17 |
| `IsUnhealthy` (677, `StoreInfo`) | 公开；函数/方法 | `pkg/statistics/store.go:143` | 无直接用例，负责任务补 oracle | 17 |
| `GetLabelValue` (682, `StoreInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:1677` | `pkg/schedule/scatter/region_scatterer_test.go:TestScatterRegions` | 24 |
| `CompareLocation` (693, `StoreInfo`) | 公开；函数/方法 | `pkg/core/store_topology.go:100` | `pkg/schedule/scatter/region_scatterer_test.go:TestScatterPlacementLabelCase` | 24 |
| `DistinctScore` (709) | 公开；函数/方法 | `pkg/schedule/placement/fit.go:432` | `pkg/core/store_test.go:TestDistinctScore` | 24 |
| `MergeLabels` (724) | 公开；函数/方法 | `server/cluster/cluster.go:1644` | 无直接用例，负责任务补 oracle | 24 |
| `StoresInfo` (760) | 公开；类型/接口 | `pkg/core/basic_cluster.go:24` | `pkg/schedule/placement/region_rule_cache_test.go:TestSetCacheDoesNotPromoteStaleFitAfterStoreRemoval` | 26 |
| `NewStoresInfo` (766) | 公开；函数/方法 | `pkg/core/basic_cluster.go:31` | `pkg/storage/storage_test.go:TestLoadStores` | 26 |
| `GetStore` (775, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:51` | `pkg/replication/replication_mode_test.go:TestAsynctimeout` | 26 |
| `GetStores` (786, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:57` | `pkg/statistics/hot_peer_cache_test.go:TestRemoveExpireItems` | 26 |
| `GetMetaStores` (797, `StoresInfo`) | 公开；函数/方法 | `tests/cluster.go:404` | `pkg/storage/storage_test.go:TestLoadStores` | 26 |
| `GetStoreIDs` (808, `StoresInfo`) | 公开；函数/方法 | `pkg/core/region.go:567` | `pkg/core/region_tree_test.go:TestRegionInfo` | 26 |
| `GetFollowerStores` (820, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:146` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 56 |
| `GetRegionStores` (833, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:144` | `server/cluster/cluster_test.go:TestRegionHeartbeat` | 56 |
| `GetLeaderStore` (846, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:147` | 无直接用例，负责任务补 oracle | 56 |
| `GetStoreCount` (856, `StoresInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:889` | `pkg/storage/storage_test.go:TestLoadStores` | 56 |
| `GetNonWitnessVoterStores` (863, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:145` | 无直接用例，负责任务补 oracle | 56 |
| `GetAvgNetworkSlowScore` (876, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:148` | `pkg/core/store_test.go:TestGetAvgNetworkSlowScore` | 26 |
| `PutStore` (909, `StoresInfo`) | 公开；函数/方法 | `tests/testutil.go:281` | `pkg/mcs/scheduling/server/server_test.go:TestRegionHeartbeatReturnsNotBootstrappedWhenHeartbeatStreamsMissing` | 27 |
| `putStoreLocked` (916, `StoresInfo`) | 内部；函数/方法 | `server/cluster/cluster.go:1165` | `pkg/core/store_test.go:TestPutStore` | 27 |
| `ResetStores` (935, `StoresInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:883` | `pkg/statistics/hot_peer_cache_test.go:TestRemoveExpireItems` | 27 |
| `PauseLeaderTransfer` (943, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:113` | `server/cluster/cluster_test.go:TestStores` | 27 |
| `ResumeLeaderTransfer` (962, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:125` | `server/cluster/cluster_test.go:TestStores` | 27 |
| `SlowStoreEvicted` (977, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:137` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 27 |
| `StoppingStoreEvicted` (990, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:145` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 27 |
| `SlowStoreRecovered` (1002, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:168` | 无直接用例，负责任务补 oracle | 27 |
| `StoppingStoreRecovered` (1015, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:175` | 无直接用例，负责任务补 oracle | 27 |
| `SlowTrendEvicted` (1029, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:153` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 27 |
| `SlowTrendRecovered` (1044, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:161` | 无直接用例，负责任务补 oracle | 27 |
| `ResetStoreLimit` (1057, `StoresInfo`) | 公开；函数/方法 | `pkg/core/store_option.go:284` | `pkg/schedule/checker/rule_checker_test.go:TestRuleCheckerTestSuite` | 27 |
| `DeleteStore` (1066, `StoresInfo`) | 公开；函数/方法 | `server/api/store.go:83` | `pkg/statistics/store_test.go:TestFilterUnhealthyStore` | 27 |
| `UpdateStoreStatus` (1073, `StoresInfo`) | 公开；函数/方法 | `pkg/core/basic_cluster.go:36` | `pkg/schedule/schedulers/scatter_range_test.go:TestScatterRangeBalance` | 27 |
| `TriggerNetworkSlowEvict` (1089, `StoresInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:2994` | `pkg/schedule/schedulers/evict_slow_store_test.go:TestEvictSlowStoreTestSuite` | 27 |
| `ResetTriggerNetworkSlowEvict` (1101, `StoresInfo`) | 公开；函数/方法 | `server/cluster/cluster.go:3022` | 无直接用例，负责任务补 oracle | 27 |
| `IsStoreContainLabel` (1113) | 公开；函数/方法 | `server/grpc_service.go:863` | 无直接用例，负责任务补 oracle | 13/24 |
| `getStoreLabelValue` (1119) | 内部；函数/方法 | `pkg/core/store_topology.go:34` | 无直接用例，负责任务补 oracle | 13/24 |
| `IsAvailableForMinResolvedTS` (1129) | 公开；函数/方法 | `server/cluster/cluster.go:2649` | 无直接用例，负责任务补 oracle | 13/24 |
| `ValidateStoreEngineKey` (1137) | 公开；函数/方法 | `server/grpc_service.go:856` | 无直接用例，负责任务补 oracle | 13/24 |

### store_option.go

源文件：[store_option.go](../../pkg/core/store_option.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `StoreCreateOption` (29) | 公开；类型/接口 | `pkg/core/store.go:88` | `pkg/core/store_test.go:TestPutStore` | 11–13/23 |
| `SetStoreAddress` (32) | 公开；函数/方法 | `server/cluster/cluster.go:1647` | 无直接用例，负责任务补 oracle | 11–13/23 |
| `SetStoreLabels` (43) | 公开；函数/方法 | `client/http/interface.go:57` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestSelectLeader` | 11–13/23 |
| `SetStoreStartTime` (52) | 公开；函数/方法 | `server/cluster/cluster.go:1650` | 无直接用例，负责任务补 oracle | 17 |
| `SetStoreVersion` (61) | 公开；函数/方法 | `server/cluster/cluster.go:1648` | 无直接用例，负责任务补 oracle | 11–13/23 |
| `SetStoreDeployPath` (71) | 公开；函数/方法 | `server/cluster/cluster.go:1651` | 无直接用例，负责任务补 oracle | 11–13/23 |
| `SetStoreState` (80) | 公开；函数/方法 | `tests/testutil.go:277` | `pkg/statistics/store_load_test.go:TestHistoryGCDoesNotReaddRemovedStore` | 11–13/23 |
| `SetNodeState` (105) | 公开；函数/方法 | `tests/testutil.go:278` | `pkg/progress/progress_test.go:TestOnlineAndOffline` | 11–13/23 |
| `PauseLeaderTransfer` (114) | 公开；函数/方法 | `pkg/core/store.go:941` | `server/cluster/cluster_test.go:TestStores` | 18 |
| `ResumeLeaderTransfer` (126) | 公开；函数/方法 | `pkg/core/store.go:960` | `server/cluster/cluster_test.go:TestStores` | 18 |
| `SlowStoreEvicted` (139) | 公开；函数/方法 | `pkg/core/store.go:975` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 18 |
| `StoppingStoreEvicted` (147) | 公开；函数/方法 | `pkg/core/store.go:988` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 18 |
| `SlowTrendEvicted` (155) | 公开；函数/方法 | `pkg/core/store.go:1027` | `pkg/schedule/schedulers/hot_region_test.go:TestHotReadRegionScheduleSkipsLeaderEvictedMoveLeaderTarget` | 18 |
| `SlowTrendRecovered` (162) | 公开；函数/方法 | `pkg/core/store.go:1043` | 无直接用例，负责任务补 oracle | 18 |
| `SlowStoreRecovered` (169) | 公开；函数/方法 | `pkg/core/store.go:1001` | 无直接用例，负责任务补 oracle | 18 |
| `StoppingStoreRecovered` (176) | 公开；函数/方法 | `pkg/core/store.go:1014` | 无直接用例，负责任务补 oracle | 18 |
| `SetLeaderCount` (183) | 公开；函数/方法 | `pkg/core/store.go:1077` | `pkg/statistics/store_collection_test.go:TestStoreStatistics` | 20/23 |
| `SetRegionCount` (190) | 公开；函数/方法 | `pkg/core/store.go:1078` | `pkg/core/store_test.go:TestNewStore` | 20/23 |
| `SetLearnerCount` (197) | 公开；函数/方法 | `pkg/core/store.go:1080` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetWitnessCount` (204) | 公开；函数/方法 | `pkg/core/store.go:1079` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetPendingPeerCount` (211) | 公开；函数/方法 | `pkg/core/store.go:1081` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetLeaderSize` (218) | 公开；函数/方法 | `pkg/core/store.go:1082` | `server/cluster/cluster_test.go:TestUpdateAndDeleteLabel` | 20/23 |
| `SetRegionSize` (225) | 公开；函数/方法 | `pkg/core/store.go:1083` | `pkg/core/store_test.go:TestRegionScore` | 20/23 |
| `SetLeaderWeight` (232) | 公开；函数/方法 | `server/cluster/cluster.go:2002` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetRegionWeight` (239) | 公开；函数/方法 | `server/cluster/cluster.go:2003` | 无直接用例，负责任务补 oracle | 20/23 |
| `SetLastHeartbeatTS` (246) | 公开；函数/方法 | `tests/testutil.go:279` | `pkg/unsaferecovery/unsafe_recovery_controller_test.go:TestRemoveFailedStores` | 17 |
| `SetLastPersistTime` (253) | 公开；函数/方法 | `server/cluster/cluster.go:1236` | 无直接用例，负责任务补 oracle | 17 |
| `SetStoreStats` (260) | 公开；函数/方法 | `tests/testutil.go:272` | `pkg/core/store_stats_test.go:TestStoreStats` | 14/15/16 |
| `SetNewStoreStats` (267) | 公开；函数/方法 | `pkg/schedule/schedulers/range_cluster.go:63` | 无直接用例，负责任务补 oracle | 14/15/16 |
| `SetMinResolvedTS` (278) | 公开；函数/方法 | `server/grpc_service.go:2686` | `tests/integrations/client/http_client_test.go:TestHTTPClientTestSuiteWithServiceDiscovery` | 17 |
| `ResetStoreLimit` (285) | 公开；函数/方法 | `pkg/core/store.go:1056` | `pkg/schedule/checker/rule_checker_test.go:TestRuleCheckerTestSuite` | 19 |
| `SetStoreLimit` (298) | 公开；函数/方法 | `server/handler.go:270` | `pkg/statistics/store_collection_test.go:TestStoreLimitMetricsIncludeTransferLeaderIn` | 19 |
| `SetLastAwakenTime` (307) | 公开；函数/方法 | `server/cluster/cluster.go:1216` | `server/cluster/cluster_test.go:TestAwakenStore` | 17 |
| `SetNetworkSlowTriggers` (314) | 公开；函数/方法 | `pkg/core/store.go:1094` | 无直接用例，负责任务补 oracle | 11–13/23 |
| `SetStoreMeta` (322) | 公开；函数/方法 | `pkg/mcs/router/server/meta/watcher.go:81` | 无直接用例，负责任务补 oracle | 11–13/23 |

### store_stats.go

源文件：[store_stats.go](../../pkg/core/store_stats.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `DFSStats` (26) | 公开；类型/接口 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 14/16 |
| `storeStats` (31) | 内部；类型/接口 | `pkg/core/store.go:65` | `pkg/statistics/store_collection_test.go:TestStoreStatistics` | 14/16 |
| `newStoreStats` (39) | 内部；函数/方法 | `pkg/core/store.go:91` | 无直接用例，负责任务补 oracle | 15 |
| `updateRawStats` (46, `storeStats`) | 内部；函数/方法 | `pkg/core/store_option.go:262` | 无直接用例，负责任务补 oracle | 14/16 |
| `GetStoreStats` (57, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:580` | `tests/integrations/mcs/scheduling/server_test.go:TestServerTestSuite` | 14/16 |
| `CloneStoreStats` (64, `storeStats`) | 公开；函数/方法 | `pkg/schedule/schedulers/range_cluster.go:59` | 无直接用例，负责任务补 oracle | 14/16 |
| `GetCapacity` (72, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:480` | `pkg/core/store_stats_test.go:TestStoreStats` | 14/16 |
| `GetAvailable` (79, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:478` | `pkg/core/store_stats_test.go:TestStoreStats` | 14/16 |
| `GetUsedSize` (86, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:479` | `pkg/core/store_stats_test.go:TestStoreStats` | 14/16 |
| `GetBytesWritten` (93, `storeStats`) | 公开；函数/方法 | `pkg/core/region.go:226` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 14/16 |
| `GetBytesRead` (100, `storeStats`) | 公开；函数/方法 | `pkg/core/region.go:228` | `pkg/statistics/hot_peer_cache_test.go:TestCacheInherit` | 14/16 |
| `GetKeysWritten` (107, `storeStats`) | 公开；函数/方法 | `pkg/core/region.go:227` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 14/16 |
| `GetKeysRead` (114, `storeStats`) | 公开；函数/方法 | `pkg/core/region.go:229` | `pkg/statistics/utils/kind_test.go:TestGetLoads` | 14/16 |
| `IsBusy` (121, `storeStats`) | 公开；函数/方法 | `pkg/response/store.go:74` | `pkg/schedule/filter/filters_test.go:TestStoreStateFilter` | 14/16 |
| `GetSendingSnapCount` (128, `storeStats`) | 公开；函数/方法 | `pkg/response/store.go:124` | 无直接用例，负责任务补 oracle | 14/16 |
| `GetReceivingSnapCount` (135, `storeStats`) | 公开；函数/方法 | `pkg/response/store.go:125` | 无直接用例，负责任务补 oracle | 14/16 |
| `GetAvgAvailable` (142, `storeStats`) | 公开；函数/方法 | `pkg/core/store.go:521` | `pkg/core/store_stats_test.go:TestStoreStats` | 15 |
| `climp0` (151) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 15 |

### store_topology.go

源文件：[store_topology.go](../../pkg/core/store_topology.go)。

| 符号（行） | 可见性/用途 | 引用代表 | Go 测试静态证据 | 负责任务 |
| --- | --- | --- | --- | --- |
| `GetStoreTopoWeight` (24) | 公开；函数/方法 | `server/cluster/cluster.go:2175` | `pkg/core/store_topology_test.go:TestTopologyWeight` | 25 |
| `buildTopology` (60) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/store_topology_test.go:TestTopologyWeightUsesCaseSensitiveLabelKeys` | 25 |
| `updateTopology` (109) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 25 |
| `cleanTopology` (126) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 25 |
| `getTopology` (136) | 内部；函数/方法 | 本文件内部/未见外部引用 | 无直接用例，负责任务补 oracle | 25 |
| `putTopology` (142) | 内部；函数/方法 | 本文件内部/未见外部引用 | `pkg/core/store_topology_test.go:TestTopologyWeightUsesCaseSensitiveLabelKeys` | 25 |

根包共 12 个生产文件、499 个函数/类型声明；以下补齐非函数声明与接口方法。

### 常量、变量与接口方法补表

这些声明同属上述源文件；调用方/测试沿用拥有类型或文件各行，常量边界需对应任务新增 oracle。

| 源文件 | 声明 | 可见性/用途、Rust 缺口与责任 |
| --- | --- | --- |
| `basic_cluster.go:97` RegionSetInformer | `GetTotalRegionCount`, `RandFollowerRegions`, `RandLeaderRegions`, `RandLearnerRegions`, `RandWitnessRegions`, `RandPendingRegions`, `GetAverageRegionSize`, `GetStoreRegionCount`, `GetRegion`, `GetAdjacentRegions`, `ScanRegions`, `GetRegionByKey`, `BatchScanRegions`, `GetRegionCount`, `GetStoreLeaderCountByRange`, `GetStoreLearnerCountByRange`, `GetStorePeerCountByRange` | 大写公开、其余内部；根包 Rust 均缺失；26/27/50–58；informer/controller 与扫描选项 |
| `basic_cluster.go:140` StoreSetInformer | `GetStores`, `GetStore`, `GetRegionStores`, `GetNonWitnessVoterStores`, `GetFollowerStores`, `GetLeaderStore`, `GetAvgNetworkSlowScore` | 大写公开、其余内部；根包 Rust 均缺失；26/27/50–58；informer/controller 与扫描选项 |
| `basic_cluster.go:152` StoreSetController | `PauseLeaderTransfer`, `ResumeLeaderTransfer`, `SlowStoreEvicted`, `StoppingStoreEvicted`, `SlowStoreRecovered`, `StoppingStoreRecovered`, `SlowTrendEvicted`, `SlowTrendRecovered` | 大写公开、其余内部；根包 Rust 均缺失；26/27/50–58；informer/controller 与扫描选项 |
| `factory.go:50` var | `RegionFactory`, `RegionPeerFactory`, `RegionHeartbeatResponseFactory`, `PeerStatsFactory`, `StoreFactory`, `StoreStatsFactory`, `KeyDictionaryFactory`, `MemberFactory`, `ClusterFactory`, `TimeIntervalFactory`, `QueryStatsFactory`, `CPUStatsFactory` | 大写公开、其余内部；根包 Rust 均缺失；16/29/60/61；工厂不迁移，其他属后续主题 |
| `metrics.go:127` var | `HeartbeatBreakdownHandleDurationSum`, `HeartbeatBreakdownHandleCount`, `AcquireRegionsLockWaitDurationSum`, `AcquireRegionsLockWaitCount`, `waitRegionsLockDurationSum`, `waitRegionsLockCount`, `waitSubRegionsLockDurationSum`, `waitSubRegionsLockCount`, `preCheckDurationSum`, `preCheckCount`, `asyncHotStatsDurationSum`, `asyncHotStatsCount`, `regionGuideDurationSum`, `regionGuideCount`, `checkOverlapsDurationSum`, `checkOverlapsCount`, `validateRegionDurationSum`, `validateRegionCount`, `setRegionDurationSum`, `setRegionCount`, `updateSubTreeDurationSum`, `updateSubTreeCount`, `regionCollectDurationSum`, `regionCollectCount`, `otherDurationSum`, `otherCount`, `queryRegionDuration`, `queryRegionCount`, `queryRegionByKeysDuration`, `queryRegionByPrevKeysDuration`, `queryRegionByIDsDuration`, `queryRegionKeysCount`, `queryRegionPrevKeysCount`, `queryRegionIDsCount`, `bucketEventCounter`, `versionStaleCounter`, `versionNotChangeCounter`, `RegionCacheMissCounter`, `UpdateFailedCounter`, `UpdateSuccessCounter` | 大写公开、其余内部；根包 Rust 均缺失；59；其他指标属后续宿主主题 |
| `metrics.go:139` 单项 | `tracerPool` | 大写公开、其余内部；根包 Rust 均缺失；59；其他指标属后续宿主主题 |
| `metrics.go:146` RegionHeartbeatProcessTracer | `Begin`, `OnPreCheckFinished`, `OnAsyncHotStatsFinished`, `OnRegionGuideFinished`, `OnSaveCacheBegin`, `OnSaveCacheFinished`, `OnCheckOverlapsFinished`, `OnValidateRegionFinished`, `OnSetRegionFinished`, `OnUpdateSubTreeFinished`, `OnCollectRegionStatsFinished`, `OnAllStageFinished`, `Release` | 大写公开、其余内部；根包 Rust 均缺失；59；其他指标属后续宿主主题 |
| `region.go:57` const | `randomRegionMaxRetry`, `ScanRegionLimit`, `batchSearchSize`, `CollectFactor` | 大写公开、其余内部；根包 Rust 均缺失；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:114` const | `Storage`, `Sync`, `Heartbeat` | 大写公开、其余内部；根包 Rust 均缺失；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:206` const | `EmptyRegionApproximateSize`, `EmptyRegionApproximateKeys`, `ImpossibleFlowSize`, `statsReportMinInterval`, `statsReportMaxInterval`, `InitClusterRegionThreshold` | 大写公开、其余内部；根包 Rust 均缺失；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:214` RegionHeartbeatResponse | `GetTargetPeer`, `GetRegionId` | 大写公开、其余内部；根包 Rust 均缺失；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:220` RegionHeartbeatRequest | `GetTerm`, `GetRegion`, `GetLeader`, `GetDownPeers`, `GetPendingPeers`, `GetBytesWritten`, `GetKeysWritten`, `GetBytesRead`, `GetKeysRead`, `GetInterval`, `GetQueryStats`, `GetApproximateSize`, `GetApproximateKeys`, `GetBucketMeta` | 大写公开、其余内部；根包 Rust 均缺失；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:416` const | `LeaderScatter`, `PeerScatter`, `LearnerScatter`, `Unknown` | 大写公开、其余内部；根包 Rust 均缺失；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:1780` const | `AllInSubTree`, `LeaderInSubTree`, `FollowerInSubTree`, `LearnerInSubTree`, `WitnessInSubTree`, `PendingPeerInSubTree` | 大写公开、其余内部；根包 Rust 均缺失；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region.go:2285` 单项 | `defaultPollInterval` | 大写公开、其余内部；根包 Rust 均缺失；10/28/30/36/44/50–56/61；阈值、来源、规则与子树角色 |
| `region_tree.go:60` const | `defaultBTreeDegree` | 大写公开、其余内部；根包 Rust 均缺失；39；BTree degree |
| `store.go:59` const | `storePersistInterval`, `initialMinSpace`, `slowStoreThreshold`, `awakenStoreInterval`, `EngineKey`, `EngineRoleKey`, `EngineTiFlash`, `EngineTiFlashCompute`, `EngineTiKV` | 大写公开、其余内部；根包 Rust 均缺失；11–13/17/20–24；状态阈值与引擎名 |
| `store.go:445` 单项 | `minWeight` | 大写公开、其余内部；根包 Rust 均缺失；11–13/17/20–24；状态阈值与引擎名 |
| `store.go:446` 单项 | `maxScore` | 大写公开、其余内部；根包 Rust 均缺失；11–13/17/20–24；状态阈值与引擎名 |
| `store.go:667` var | `storeDisconnectDuration`, `storeUnhealthyDuration` | 大写公开、其余内部；根包 Rust 均缺失；11–13/17/20–24；状态阈值与引擎名 |
| `store.go:705` 单项 | `replicaBaseScore` | 大写公开、其余内部；根包 Rust 均缺失；11–13/17/20–24；状态阈值与引擎名 |
| `store_topology.go:49` 单项 | `topologyPool` | 大写公开、其余内部；根包 Rust 均缺失；25；pool 是分配优化 |

## 调用链和验证前置条件

- [server/grpc_service.go](../../server/grpc_service.go) 和 [MCS grpc_service.go](../../pkg/mcs/scheduling/server/grpc_service.go)
  从协议心跳构建 Region，61 提供转换；网络服务接入仍属后续主题。
- [server/cluster/cluster.go](../../server/cluster/cluster.go) 与 [MCS cluster.go](../../pkg/mcs/scheduling/server/cluster.go)
  使用 guide、root/subtree、bucket CAS 和 tracer；35/46–48/59 提供所需契约，心跳整体管线不在本次范围。
- [schedule](../../pkg/schedule) 的 filter/operator/placement 消费 Peer、Store score、label、随机 Region 与 informer；
  9/18–25/50–58 提供领域能力，完整调度策略不迁移。
- [statistics](../../pkg/statistics)、[response](../../pkg/response)、[server/api](../../server/api)
  消费快照、统计和编码；领域汇总由 30/31/53/54/58 负责，响应由 62–67，其他统计采集属于后续主题。
- [router watcher](../../pkg/mcs/router/server/meta/watcher.go) 构造 Store/BasicCluster；
  11/26/57 提供领域 API，watch/storage 装配不属于 core。

后续任务必须使用当前 Go 工具链和真实 oracle，新增 shared JSON，不得从 Rust 输出反写期望。
Go failpoint 验证串行、结束后禁用；Cargo 按任务领取共享槽位并设绝对 CARGO_TARGET_DIR。
时间用显式时钟，随机验证候选性质；clone 需要区分元信息深拷贝、统计与 limiter 共享。
async RateLimiter、Runner 不能当成同名同步 Go 接口；类型适配由负责任务独立红绿验证。
未发现阻塞本次文档交付的必需依赖；尚未实现的业务都明确留给后续编号任务。

## 本次验证证据

仓库根执行以下命令，退出码均为 0：

```sh
rg -n '^func |^type |failpoint\.' pkg/core -g '*.go'
rg -n 'concurrentBucketHeartbeat|UpdateSubTree' pkg/core server/cluster -g '*.go'
rg -n 'core\.(NewBasicCluster|RegionFromHeartbeat|NewStoreInfo|GenerateRegionGuideFunc|NewHeartbeatProcessTracer)|\.CompareAndSetReportBuckets\(' server pkg/mcs pkg/schedule -g '*.go'
git diff 0820f0b67 -- pkg/core ':!pkg/core/storelimit'
git diff --check
```

另对本文全部 Markdown 链接、引用文件路径、根包函数/类型/接口/常量/变量声明逐一做本地存在性与覆盖检查。
根包 Go 基准差异为空；499 个函数/类型声明全部列入矩阵，非函数声明和接口方法由补表覆盖。
有效执行行为测试数 **0**；本任务为文档审计，按要求未执行 Go/Cargo，不将静态源码核验算成测试通过。
后续 68/69 必须更新实际 Rust 覆盖与运行结果。
