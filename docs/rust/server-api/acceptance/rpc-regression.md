# Classic PD 协议回归

本轮从固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4` 的
`pdpb.PD` 和 [wire 清单](../wire-inventory.md) 独立核对 56 个方法。
每个方法映射到一个实际执行的场景，机器回执逐行保存 Go 源码位置、源文件和
清单行摘要。回执键对应 [task-22.json.gz](task-22.json.gz) 的 `receipts`；
共享回执中的每个场景都有独立的命令、日志、退出码和有效测试数量。
每个场景实际通过 1 项，0 失败、0 忽略。

| 场景 | 本轮实际方法 | 回执键 |
| --- | --- | --- |
| `members` | GetMembers, GetClusterInfo | `compat-readonly-1794153303328260000.json` |
| `classic-tso` | Tso, GetMinTS | `compat-readonly-1794153303328260000.json` |
| `bootstrap` | Bootstrap, IsBootstrapped | `compat-readonly-1794153303328260000.json` |
| `alloc-id` | AllocID | `compat-readonly-1794153303328260000.json` |
| `store-registration` | GetStore, PutStore, GetAllStores | `compat-readonly-1794153303328260000.json` |
| `cluster-host` | StoreHeartbeat, RegionHeartbeat | `compat-readonly-1794153303328260000.json` |
| `bucket-heartbeat` | ReportBuckets | `compat-readonly-1794153303328260000.json` |
| `sync-regions` | SyncRegions | `compat-readonly-1794153303328260000.json` |
| `region-lookup` | GetRegion, GetPrevRegion, GetRegionByID | `compat-readonly-1794153303328260000.json` |
| `region-scan` | ScanRegions, BatchScanRegions | `compat-readonly-1794153303328260000.json` |
| `region-query` | QueryRegion | `compat-readonly-1794153303328260000.json` |
| `ask-split` | AskSplit, AskBatchSplit | `compat-readonly-1794153303328260000.json` |
| `report-split` | ReportSplit, ReportBatchSplit | `compat-readonly-1794153303328260000.json` |
| `cluster-config` | GetClusterConfig, PutClusterConfig | `compat-readonly-1794153303328260000.json` |
| `scatter` | ScatterRegion, GetOperator | `compat-readonly-1794153303328260000.json` |
| `split-regions` | SplitRegions | `compat-readonly-1794153303328260000.json` |
| `split-scatter` | SplitAndScatterRegions | `compat-readonly-1794153303328260000.json` |
| `global-config` | StoreGlobalConfig, LoadGlobalConfig | `compat-readonly-1794153303328260000.json` |
| `global-watch` | WatchGlobalConfig | `compat-readonly-1794153303328260000.json` |
| `report-min-resolved-ts` | ReportMinResolvedTS | `compat-readonly-1794153303328260000.json` |
| `external-timestamp` | SetExternalTimestamp, GetExternalTimestamp | `compat-readonly-1794153303328260000.json` |
| `deprecated-rpc` | SyncMaxTS, GetDCLocationInfo, WatchGCSafePointV2, WatchGCStates | `compat-readonly-1794153303328260000.json` |
| `admin-markers` | IsSnapshotRecovering | `http-admin-markers-oracle-1794154442161548000.json` |
| `gc-legacy` | GetGCSafePoint, UpdateGCSafePoint, UpdateServiceGCSafePoint | `compat-readonly-1794153303328260000.json` |
| `gc-v2` | GetGCSafePointV2, UpdateGCSafePointV2, UpdateServiceSafePointV2, GetAllGCSafePointV2 | `compat-readonly-1794153303328260000.json` |
| `gc-advance` | AdvanceTxnSafePoint, AdvanceGCSafePoint | `compat-readonly-1794153303328260000.json` |
| `gc-barrier` | SetGCBarrier, DeleteGCBarrier | `compat-readonly-1794153303328260000.json` |
| `gc-global-barrier` | SetGlobalGCBarrier, DeleteGlobalGCBarrier | `compat-readonly-1794153303328260000.json` |
| `gc-state` | GetGCState, GetAllKeyspacesGCStates | `compat-readonly-1794153303328260000.json` |

前 23 组沿用 [任务 100](../task-100.md) 的原断言。部分场景用 Rust
`PdNodeBuilder` 启动实际 TCP 服务。六个 GC 场景使用独立的 Go 和 Rust PD
进程，覆盖真实 follower、请求取消、适用的事务故障和真实 etcd WAL 重启。
各场景的两端 RPC 观测及物理键、值和修订号保存在回执 `evidence`。
两项废弃 GC watch 继续校验完整 `Unimplemented` 状态和消息。
原有成员发现后端错误的两条完整消息仍分别校验。独立时钟和集群身份按各原场景
已经审查的规则比较，原始观测单独保留。

Go 客户端回归单独使用 `go-client-core`。三个已有 Go 工具分别驱动心跳、
Region 查询和 TSO，并验证切换 leader 后现有客户端重新发现 leader、继续请求，
以及服务端观察到实际 bootstrap 和注册。额外 gRPC 服务由
[批次 16](extra-services.md) 独立交接。
