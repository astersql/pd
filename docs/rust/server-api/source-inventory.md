# 服务依赖与来源覆盖基准

盘点日期：2026-10-28（Asia/Shanghai）。工作树基准 HEAD：`bdec774cc277fc2bb65ffa44cfbfb1849bfbe0e2`。
只读计划基线 `5d389785abd44b5bbd9d826b24bc454e6e7e2f5c` 的后继提交；本清单只固定事实和验收责任，不宣布主题7实现完成。
任务1无前置批次。任务2/3尚无产出，因此本清单的协议、HTTP、配置及指标归属只是来源入口，后续须展开精确契约。

## 集合与口径

来源集合共 **1909** 个文件；按仓库相对路径去重、Unicode排序；SHA-256取磁盘原始字节，不归一化。末尾完整manifest串按 `sha256 + 两空格 + path + LF` 拼接，集合摘要为 `a6765e82a34496928c803b5cd616955b17c430bd5ebd243aa16239c9c5cdee91`。
集合包含来源主题7全部目录、102项任务上下文实际存在的文件/目录、keypath持久化定义、领域依赖及架构/规则。测试、manifest、说明文件同样纳入哈希。拟建文件不纳入存在集合。
owner为本计划验收责任编号，可有多个；102是最终来源审计责任，不意味着它负责实现所有领域。源码行号绑定当前HEAD；声明扫描用于定位，不是语义等价证据。

## 准入结论与领域边界

| 边界 | 实际能力与公开入口 | owner / 准入要求 | Go源码事实 | 冻结输出 | 实际运行证据 |
| --- | --- | --- | --- | --- | --- |
| 主题6调度内核 | `BasicCluster`、`Coordinator::new/init_schedulers`、`SchedulerController`、`MetaStore`；见 `pkg/core/basic_cluster.rs`、`pkg/schedule/coordinator.rs`、`pkg/storage/domain/lib.rs` | 5–17、29–31、62–74；可复用领域能力，服务装配仍缺失 | `server/cluster/cluster.go`、`pkg/schedule/coordinator.go` | `docs/rust/schedule-execution/fixtures/task_106.json`、`fixtures/task_156.json`（均相对schedule-execution） | task-156历史验收；当前仅校验日志哈希，未重跑 |
| 心跳/同步适配 | `PdServices`可选服务、`store_heartbeat_adapter`、`region_heartbeat_adapter`、`bucket_heartbeat_adapter`、`sync_transport::RegionSyncer` | 5、8–11、95；runtime的Application未覆盖这些可选方法，默认None，不能宣称已装配 | `server/grpc_service.go`、`server/cluster/` | 本任务未新增冻结响应 | 既有独立测试文件存在不等于已运行；需task-008至011新证据 |
| 最小运行时 | `bootstrap_config`、`PdNodeBuilder::new/start`、`PdNode::ready/wait/shutdown` | 6、7、83–98；现有单节点领导权/ID/TSO维护及同端口router不证明完整生命周期 | `server/server.go` | 无完整Go进程冻结输出 | task-156不涵盖主题7全部生命周期；阻塞完整服务准入 |
| 主题8领域服务 | GC、keyspace、resource、replication等须复用各自真实领域服务 | 37、49、77、78、100、102；领域方法及故障证据未逐项确认前，保持未准入 | `server/apiv2/handlers/`及对应Go领域层 | 未确认可复用完整输出 | 缺主题8逐项运行验收，禁止在适配层固定响应或重写领域算法 |
| 主题9运维 | 管理恢复/日志/历史热点/诊断等由领域与运维实现提供 | 61、66、73–76、92、102；只接线经过确认的API | `server/api/`、`pkg/utils/logutil/` | 未确认完整运维输出 | 缺主题9逐项运行证据；接口不得以同名crate视为完成 |
| 主题10元数据拓扑 | 当前`MetaStore`+外部etcd是过渡模式；Go `embed.StartEtcd`不等价 | 88、89、98、102；原生后端/embedded-etcd生产等价性保持阻塞 | `server/server.go`、`server/join/join.go` | 无等价拓扑冻结输出 | 缺durability与拓扑证据，不在本任务扩建后端 |
| 只读Go兼容入口 | `scripts/run-pd-compat.sh`、`pkg/proto/build.rs`的Go调用链待任务4审计 | 4、99–102；禁直接执行旧脚本或make/failpoint入口 | failpoint注入需改写源码，当前禁止 | 仅可复用已核验fixture/既有二进制 | 本次Go运行差分0；主题6的局部授权不外推为主题7免除差分 |

## 当前真实注册与调用关系

`pkg/server/lib.rs::router`把生成的`PdServer<SyncGrpc<Grpc>>`合并到axum；`pkg/server/http.rs::router`仅显式注册health和members两条GET。独立schedule_admin模块不等于已挂上主HTTP路由。
`pkg/runtime/node.rs::initialize_node`创建Member、campaign、Allocator和Oracle，然后传入Application。Application只实现identity/healthy/alloc_id/timestamp；host未提供共享集群/心跳/同步对象。50ms oracle更新和1s lease renew由一个tokio worker驱动。
`Coordinator`已有领域装配入口与scatter/splitter访问器；此处未见runtime调用，不以可构造代替后台调度已运行。现有测试入口包括`pkg/runtime/node_test.rs`、server各独立`*_test.rs`和`tests/rust-compat/Cargo.toml`，须按后续任务显式注册并取得行为证据。
kvproto固定commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`；build.rs固定版本 `v0.0.0-20261016101550-f8ad079c8e98` 及ZIP SHA-256 `aff09d074d493874bfd7e3e63c766a8d93f42901ff235dde2d2ced0c7f2a615e`。本次未调用该构建脚本。

## 102项来源、现状及验收映射

每行的来源引用均取任务上下文；目录会展开进入末尾manifest。目标注明存在/拟建；存在不代表可用。每项需同时验证成功、错误、取消及持久化副作用（适用时）。
任务证据位置约定为`docs/rust/server-api/task-NNN.md`；除task-001外，当前均缺本计划实际运行证据。任务4先提供只读入口，后续差分不得绕过它。

| 编号 / owner | Go或现有Rust来源入口 | Rust目标当前状态 | 准入 / 所需验证 | 实际证据 |
| --- | --- | --- | --- | --- |
| 1 固定服务依赖与覆盖基准 | `server/server.go`、`server/cluster/`、`pkg/server/lib.rs`、`pkg/runtime/node.rs`、`docs/rust/schedule-execution/task-156.md` | 本任务文档；后续协议/配置清单见2/3 | 无前置；只读文档核验 | 见task-001；仅盘点 |
| 2 核对协议与HTTP路由归属 | `pkg/proto/build.rs`、`pkg/proto/lib.rs`、`server/grpc_service.go`、`server/api/router.go`、`server/apiv2/router.go`、`server/apiv2/handlers/` | 本任务文档；后续协议/配置清单见2/3 | 依赖1；只读契约核对 | 待补task-002；未运行 |
| 3 固定配置指标与命令契约 | `server/config/`、`pkg/basicserver/`、`cmd/pd-server/main.go`、`server/api/service_middleware.go`、`pkg/server/`、`pkg/runtime/` | 本任务文档；后续协议/配置清单见2/3 | 依赖1；只读契约核对 | 待补task-003；未运行 |
| 4 建立Go只读兼容验证入口 | `scripts/run-pd-compat.sh`、`pkg/proto/build.rs`、`pkg/storage/kv/etcd_fixture_test.rs`、`tests/rust-compat/harness.rs` | `tests/rust-compat/readonly_oracle.rs`（拟建）、`tests/rust-compat/readonly_oracle_test.rs`（拟建） | 依赖1–3；只读调用链与文件集复核 | 待补task-004；未运行 |
| 5 接入共享集群状态 | `server/cluster/state_pipeline.rs`、`pkg/server/lib.rs`、`pkg/runtime/node.rs` | `pkg/runtime/cluster_host.rs`、`pkg/runtime/cluster_host_test.rs` | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 见[task-005](task-005.md)：共享状态、真实心跳/持久化、故障清理及只读Go差分 |
| 6 恢复持久化集群视图 | `server/cluster/cluster.go`、`server/cluster/state_pipeline.rs`、`pkg/storage/endpoint/` | `pkg/runtime/cluster_restore.rs`（拟建）、`pkg/runtime/cluster_restore_test.rs`（拟建） | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 待补task-006；未运行 |
| 7 驱动多成员角色切换 | `server/server.go`、`pkg/member/`、`pkg/runtime/node.rs` | `pkg/runtime/member_role_worker.rs`（拟建）、`pkg/runtime/member_role_worker_test.rs`（拟建） | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 待补task-007；未运行 |
| 8 装配Store心跳服务 | `pkg/server/store_heartbeat_adapter.rs`、`server/grpc_service.go` | `pkg/runtime/wire_store_heartbeat.rs`（拟建）、`pkg/runtime/wire_store_heartbeat_test.rs`（拟建） | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 待补task-008；未运行 |
| 9 装配Region心跳服务 | `pkg/server/region_heartbeat_adapter.rs`、`server/cluster/heartbeat_effects.rs` | `pkg/runtime/wire_region_heartbeat.rs`（拟建）、`pkg/runtime/wire_region_heartbeat_test.rs`（拟建） | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 待补task-009；未运行 |
| 10 装配Bucket心跳服务 | `pkg/server/bucket_heartbeat_adapter.rs`、`server/cluster/bucket_heartbeat.rs` | `pkg/runtime/wire_bucket_heartbeat.rs`（拟建）、`pkg/runtime/wire_bucket_heartbeat_test.rs`（拟建） | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 待补task-010；未运行 |
| 11 装配Region同步服务 | `pkg/server/sync_transport.rs`、`pkg/syncer/`、`server/grpc_service.go` | `pkg/runtime/wire_sync_regions.rs`（拟建）、`pkg/runtime/wire_sync_regions_test.rs`（拟建） | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 待补task-011；未运行 |
| 12 驱动Store状态检查worker | `server/cluster/cluster.go`、`pkg/schedule/coordinator.go`、`server/cluster/state_pipeline.rs` | `server/cluster/store_state_worker.rs`（拟建）、`server/cluster/store_state_worker_test.rs`（拟建） | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 待补task-012；未运行 |
| 13 驱动Store统计更新worker | `server/cluster/cluster.go`、`pkg/schedule/coordinator.go`、`server/cluster/state_pipeline.rs` | `server/cluster/store_stats_worker.rs`、`server/cluster/store_stats_worker_test.rs` | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 见[task-013](task-013.md)：周期统计、取消/join、真实心跳及只读Go差分 |
| 14 驱动Store配置同步worker | `server/cluster/cluster.go`、`pkg/schedule/coordinator.go`、`server/cluster/state_pipeline.rs` | `server/cluster/store_config_worker.rs`（拟建）、`server/cluster/store_config_worker_test.rs`（拟建） | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 待补task-014；未运行 |
| 15 驱动集群指标采集worker | `server/cluster/cluster.go`、`pkg/schedule/coordinator.go`、`server/cluster/state_pipeline.rs` | `server/cluster/cluster_metrics_worker.rs`、`server/cluster/cluster_metrics_worker_test.rs` | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 见[task-015](task-015.md)：10秒健康采集、真实HTTP/MetaStore、取消与Registry注销及只读Go指标差分 |
| 16 驱动ResolvedTS聚合worker | `server/cluster/cluster.go`、`pkg/schedule/coordinator.go`、`server/cluster/state_pipeline.rs` | `server/cluster/resolved_ts_worker.rs`（拟建）、`server/cluster/resolved_ts_worker_test.rs`（拟建） | 依赖1–4及前序真实服务能力；主题6领域API+真实worker取消/join、故障/恢复 | 待补task-016；未运行 |
| 17 接通调度协调器主循环 | `pkg/schedule/coordinator.rs`、`pkg/schedule/scheduler_restore.rs`、`pkg/schedule/coordinator.go` | `pkg/runtime/schedule_worker.rs`、`pkg/runtime/schedule_policy.rs`、`pkg/runtime/schedule_worker_test.rs` | 依赖1–4及前序真实服务能力；共享canonical索引、恢复实例、真实心跳命令、取消/join | 见[task-017](task-017.md)：准备门、可控tick、TCP/rebind/stale、事务故障及只读Go差分；独立patrol仍归后续任务 |
| 18 实现服务角色请求校验 | `server/grpc_service.go`、`pkg/server/grpc.rs` | `pkg/server/request_role.rs`（拟建）、`pkg/server/request_role_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-018；未运行 |
| 19 补齐集群信息与成员查询 | `server/grpc_service.go`、`pkg/member/member_info.rs` | `pkg/server/grpc_members.rs`（拟建）、`pkg/server/grpc_members_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-019；未运行 |
| 20 实现集群Bootstrap事务 | `server/grpc_service.go`、`server/cluster/cluster.go` | `pkg/server/grpc_bootstrap.rs`、`pkg/server/grpc_bootstrap_test.rs` | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | [task-020.md](task-020.md)：事务、并发、重启、故障与只读差分 |
| 21 补齐批量ID分配 | `server/grpc_service.go`、`pkg/id/` | `pkg/server/grpc_alloc_id.rs`、`pkg/server/grpc_alloc_id_test.rs` | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | [task-021.md](task-021.md)：批量、并发、重启、耗尽、事务故障、取消与只读差分 |
| 22 实现Store注册查询 | `server/grpc_service.go`、`pkg/server/core_store_adapter.rs` | `pkg/server/grpc_store.rs`、`pkg/server/grpc_store_test.rs`、`pkg/runtime/cluster_host.rs` | 依赖1–4及前序真实服务能力；固定kvproto、共享缓存与持久层 | [task-022.md](task-022.md)：真实RPC、错误、重启与只读差分 |
| 23 实现Region点查询 | `server/grpc_service.go`、`pkg/server/core_region_adapter.rs` | `pkg/server/grpc_region_lookup.rs`（拟建）、`pkg/server/grpc_region_lookup_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-023；未运行 |
| 24 实现Region范围扫描 | `server/grpc_service.go`、`pkg/core/` | `pkg/server/grpc_region_scan.rs`（拟建）、`pkg/server/grpc_region_scan_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-024；未运行 |
| 25 实现QueryRegion双向流 | `server/grpc_service.go`、`pkg/server/grpc.rs` | `pkg/server/grpc_region_query.rs`（拟建）、`pkg/server/grpc_region_query_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-025；未运行 |
| 26 实现Split标识申请 | `server/grpc_service.go`、`pkg/id/` | `pkg/server/grpc_ask_split.rs`（拟建）、`pkg/server/grpc_ask_split_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-026；未运行 |
| 27 实现Split结果上报 | `server/grpc_service.go`、`server/cluster/` | `pkg/server/grpc_report_split.rs`（拟建）、`pkg/server/grpc_report_split_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-027；未运行 |
| 28 实现集群配置RPC | `server/grpc_service.go`、`server/config/` | `pkg/server/grpc_cluster_config.rs`（拟建）、`pkg/server/grpc_cluster_config_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-028；未运行 |
| 29 实现Scatter与Operator查询 | `server/grpc_service.go`、`pkg/schedule/coordinator.rs`、`pkg/schedule/scatter/` | `pkg/server/grpc_scatter.rs`（拟建）、`pkg/server/grpc_scatter_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-029；未运行 |
| 30 实现SplitRegions请求 | `server/grpc_service.go`、`pkg/schedule/splitter/` | `pkg/server/grpc_split_regions.rs`（拟建）、`pkg/server/grpc_split_regions_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-030；未运行 |
| 31 实现SplitAndScatter请求 | `server/grpc_service.go`、`pkg/schedule/coordinator.rs` | `pkg/server/grpc_split_scatter.rs`（拟建）、`pkg/server/grpc_split_scatter_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-031；未运行 |
| 32 实现全局配置读写 | `server/grpc_service.go`、`pkg/storage/` | `pkg/server/grpc_global_config.rs`（拟建）、`pkg/server/grpc_global_config_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-032；未运行 |
| 33 实现全局配置Watch | `server/grpc_service.go`、`pkg/storage/` | `pkg/server/grpc_global_watch.rs`（拟建）、`pkg/server/grpc_global_watch_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-033；未运行 |
| 34 实现最小ResolvedTS上报 | `server/grpc_service.go`、`server/cluster/` | `pkg/server/grpc_resolved_ts.rs`（拟建）、`pkg/server/grpc_resolved_ts_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-034；未运行 |
| 35 实现外部时间戳RPC | `server/grpc_service.go`、`pkg/tso/` | `pkg/server/grpc_external_ts.rs`（拟建）、`pkg/server/grpc_external_ts_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-035；未运行 |
| 36 补齐TSO流与最小时间戳 | `server/grpc_service.go`、`pkg/tso/`、`pkg/server/grpc.rs` | `pkg/server/grpc_tso.rs`（拟建）、`pkg/server/grpc_tso_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-036；未运行 |
| 37 固定明确不支持的RPC | `server/grpc_service.go`、`pkg/proto/lib.rs` | `pkg/server/grpc_unsupported.rs`（拟建）、`pkg/server/grpc_unsupported_test.rs`（拟建） | 依赖1–4及前序真实服务能力；精确kvproto方法、错误/流/持久化差分 | 待补task-037；未运行 |
| 38 规范HTTP参数与错误映射 | `pkg/utils/apiutil/apiutil.go`、`server/api/middleware.go`、`pkg/server/http.rs` | `pkg/server/http_contract.rs`（拟建）、`pkg/server/http_contract_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-038；未运行 |
| 39 补齐地址与URL安全处理 | `pkg/utils/netutil/address.go`、`pkg/utils/tempurl/`、`server/join/client_url.go` | `pkg/runtime/net_address.rs`（拟建）、`pkg/runtime/net_address_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-039；未运行 |
| 40 实现请求上下文元数据 | `pkg/utils/requestutil/`、`pkg/utils/apiutil/serverapi/middleware.go` | `pkg/server/request_context.rs`（拟建）、`pkg/server/request_context_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-040；未运行 |
| 41 实现HTTP领导者转发 | `server/api/middleware.go`、`server/apiv2/middlewares/redirector.go`、`pkg/utils/apiutil/` | `pkg/server/http_redirect.rs`（拟建）、`pkg/server/http_redirect_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-041；未运行 |
| 42 实现gRPC领导者转发 | `server/grpc_service.go`、`pkg/utils/grpcutil/` | `pkg/server/grpc_forward.rs`（拟建）、`pkg/server/grpc_forward_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-042；未运行 |
| 43 补齐健康与就绪探针 | `server/api/health.go`、`server/apiv2/handlers/ready.go`、`pkg/basicserver/` | `pkg/server/http_health.rs`（拟建）、`pkg/server/http_health_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-043；未运行 |
| 44 接通成员管理API | `server/api/member.go`、`pkg/member/` | `pkg/server/http_member.rs`（拟建）、`pkg/server/http_member_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-044；未运行 |
| 45 接通Leader管理API | `server/api/member.go`、`pkg/member/` | `pkg/server/http_leader.rs`（拟建）、`pkg/server/http_leader_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-045；未运行 |
| 46 暴露集群状态与版本API | `server/api/cluster.go`、`server/api/status.go`、`server/api/version.go` | `pkg/server/http_cluster.rs`（拟建）、`pkg/server/http_cluster_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-046；未运行 |
| 47 暴露完整配置查询API | `server/api/config.go`、`server/config/` | `pkg/server/http_config_read.rs`（拟建）、`pkg/server/http_config_read_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-047；未运行 |
| 48 实现Schedule配置热更新 | `server/api/config.go`、`server/config/persist_schedule.rs` | `pkg/server/http_schedule_config_write.rs`（拟建）、`pkg/server/http_schedule_config_write_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-048；未运行 |
| 49 实现Replication配置热更新 | `server/api/config.go`、`server/config/persist_schedule.rs` | `pkg/server/http_replication_config_write.rs`（拟建）、`pkg/server/http_replication_config_write_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-049；未运行 |
| 50 实现服务配置热更新 | `server/api/config.go`、`server/config/persist_schedule.rs` | `pkg/server/http_server_config_write.rs`（拟建）、`pkg/server/http_server_config_write_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-050；未运行 |
| 51 实现Store查询HTTP | `server/api/store.go`、`server/api/label.go` | `pkg/server/http_store_read.rs`（拟建）、`pkg/server/http_store_read_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-051；未运行 |
| 52 实现Store状态管理HTTP | `server/api/store.go` | `pkg/server/http_store_state.rs`（拟建）、`pkg/server/http_store_state_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-052；未运行 |
| 53 实现Store标签与权重HTTP | `server/api/store.go`、`server/api/label.go` | `pkg/server/http_store_labels.rs`（拟建）、`pkg/server/http_store_labels_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-053；未运行 |
| 54 实现Store限额HTTP | `server/api/store.go`、`pkg/core/storelimit/`、`pkg/progress/` | `pkg/server/http_store_limit.rs`（拟建）、`pkg/server/http_store_limit_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-054；未运行 |
| 55 实现Region点查询HTTP | `server/api/region.go`、`pkg/response/` | `pkg/server/http_region_lookup.rs`（拟建）、`pkg/server/http_region_lookup_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-055；未运行 |
| 56 实现Region分页扫描HTTP | `server/api/region.go`、`pkg/response/` | `pkg/server/http_region_scan.rs`（拟建）、`pkg/server/http_region_scan_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-056；未运行 |
| 57 实现Region排序HTTP | `server/api/region.go`、`server/api/stats.go` | `pkg/server/http_region_top.rs`（拟建）、`pkg/server/http_region_top_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-057；未运行 |
| 58 实现Region统计HTTP | `server/api/region.go`、`server/api/stats.go` | `pkg/server/http_region_stats.rs`（拟建）、`pkg/server/http_region_stats_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-058；未运行 |
| 59 实现Region异常检查HTTP | `server/api/region.go`、`pkg/schedule/checker/` | `pkg/server/http_region_check.rs`（拟建）、`pkg/server/http_region_check_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-059；未运行 |
| 60 实现实时热点查询HTTP | `server/api/hot_status.go`、`pkg/statistics/` | `pkg/server/http_hotspot.rs`（拟建）、`pkg/server/http_hotspot_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-060；未运行 |
| 61 实现历史热点查询HTTP | `server/api/hot_status.go`、`pkg/statistics/` | `pkg/server/http_hotspot_history.rs`（拟建）、`pkg/server/http_hotspot_history_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-061；未运行 |
| 62 实现Operator只读HTTP | `server/api/operator.go`、`pkg/schedule/operator/` | `pkg/server/http_operator_read.rs`（拟建）、`pkg/server/http_operator_read_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-062；未运行 |
| 63 实现Operator管理HTTP | `server/api/operator.go`、`pkg/schedule/handler/` | `pkg/server/http_operator_write.rs`（拟建）、`pkg/server/http_operator_write_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-063；未运行 |
| 64 实现调度器生命周期HTTP | `server/api/scheduler.go`、`pkg/schedule/coordinator.rs` | `pkg/server/http_scheduler.rs`（拟建）、`pkg/server/http_scheduler_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-064；未运行 |
| 65 注册既有调度器配置路由 | `server/api/scheduler.go`、`pkg/server/schedule_admin_balance_leader.rs`、`pkg/server/schedule_admin_balance_range.rs` | `pkg/server/http_scheduler_config.rs`（拟建）、`pkg/server/http_scheduler_config_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-065；未运行 |
| 66 暴露Checker与诊断HTTP | `server/api/checker.go`、`server/api/diagnostic.go`、`pkg/schedule/diagnostic/` | `pkg/server/http_checker.rs`（拟建）、`pkg/server/http_checker_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-066；未运行 |
| 67 实现Region调度管理HTTP | `server/api/region.go`、`pkg/schedule/coordinator.rs` | `pkg/server/http_region_schedule.rs`（拟建）、`pkg/server/http_region_schedule_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-067；未运行 |
| 68 实现Placement单规则HTTP | `server/api/rule.go`、`pkg/schedule/placement/` | `pkg/server/http_rule.rs`（拟建）、`pkg/server/http_rule_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-068；未运行 |
| 69 实现Placement批量与分组HTTP | `server/api/rule.go`、`pkg/schedule/placement/` | `pkg/server/http_rule_group.rs`（拟建）、`pkg/server/http_rule_group_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-069；未运行 |
| 70 实现Region标签规则HTTP | `server/api/region_label.go`、`pkg/schedule/labeler/` | `pkg/server/http_region_label.rs`（拟建）、`pkg/server/http_region_label_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-070；未运行 |
| 71 实现Affinity查询HTTP | `server/apiv2/handlers/affinity.go`、`pkg/schedule/affinity/` | `pkg/server/http_affinity_read.rs`（拟建）、`pkg/server/http_affinity_read_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-071；未运行 |
| 72 实现Affinity更新HTTP | `server/apiv2/handlers/affinity.go`、`pkg/schedule/affinity/` | `pkg/server/http_affinity_write.rs`（拟建）、`pkg/server/http_affinity_write_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-072；未运行 |
| 73 实现Region缓存管理HTTP | `server/api/admin.go`、`server/grpc_service.go` | `pkg/server/http_admin_cache.rs`（拟建）、`pkg/server/http_admin_cache_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-073；未运行 |
| 74 实现恢复标记HTTP与RPC | `server/api/admin.go`、`server/grpc_service.go` | `pkg/server/http_admin_markers.rs`（拟建）、`pkg/server/http_admin_markers_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-074；未运行 |
| 75 实现ID恢复管理HTTP | `server/api/admin.go`、`server/api/tso.go`、`pkg/tso/` | `pkg/server/http_admin_recover_id.rs`（拟建）、`pkg/server/http_admin_recover_id_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-075；未运行 |
| 76 实现TSO恢复管理HTTP | `server/api/admin.go`、`server/api/tso.go`、`pkg/tso/` | `pkg/server/http_admin_recover_tso.rs`（拟建）、`pkg/server/http_admin_recover_tso_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-076；未运行 |
| 77 实现ResolvedTS查询HTTP | `server/api/min_resolved_ts.go`、`server/cluster/` | `pkg/server/http_resolved_ts.rs`（拟建）、`pkg/server/http_resolved_ts_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-077；未运行 |
| 78 挂接v2领域服务边界 | `server/apiv2/router.go`、`server/apiv2/handlers/`、`.plans/2026-10-22-pd-rust-渐进迁移主题/8-迁移控制面服务与MCS.md` | `pkg/server/http_v2_boundary.rs`（拟建）、`pkg/server/http_v2_boundary_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-078；未运行 |
| 79 实现HTTP服务限流 | `server/api/service_middleware.go`、`server/api/middleware.go`、`pkg/ratelimit/` | `pkg/server/http_service_limit.rs`（拟建）、`pkg/server/http_service_limit_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-079；未运行 |
| 80 实现gRPC服务限流 | `server/api/service_middleware.go`、`server/api/middleware.go`、`pkg/ratelimit/` | `pkg/server/grpc_service_limit.rs`（拟建）、`pkg/server/grpc_service_limit_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-080；未运行 |
| 81 接入服务访问审计 | `server/api/router.go`、`server/api/middleware.go`、`pkg/utils/requestutil/`、`pkg/audit/` | `pkg/server/service_audit.rs`（拟建）、`pkg/server/service_audit_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-081；未运行 |
| 82 校验服务认证与权限 | `pkg/utils/apiutil/`、`pkg/utils/grpcutil/`、`server/server.go` | `pkg/server/service_auth.rs`（拟建）、`pkg/server/service_auth_test.rs`（拟建） | 依赖1–4及前序真实服务能力；HTTP/中间件契约、字段/状态码及领域副作用 | 待补task-082；未运行 |
| 83 接入TLS监听 | `pkg/utils/grpcutil/`、`server/config/`、`pkg/runtime/node.rs` | `pkg/runtime/tls_listener.rs`（拟建）、`pkg/runtime/tls_listener_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-083；未运行 |
| 84 接入双向TLS校验 | `pkg/utils/grpcutil/`、`server/config/`、`pkg/runtime/node.rs` | `pkg/runtime/mtls_listener.rs`（拟建）、`pkg/runtime/mtls_listener_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-084；未运行 |
| 85 支持TLS材料更新 | `pkg/utils/grpcutil/`、`server/config/`、`pkg/runtime/` | `pkg/runtime/tls_reload.rs`（拟建）、`pkg/runtime/tls_reload_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-085；未运行 |
| 86 导出兼容Prometheus指标 | `pkg/basicserver/metrics.go`、`pkg/utils/metricutil/`、`server/metrics.go`、`pkg/runtime/node.rs` | `pkg/server/service_metrics.rs`（拟建）、`pkg/server/service_metrics_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-086；未运行 |
| 87 实现同端口协议装配 | `pkg/server/lib.rs`、`pkg/server/http.rs`、`pkg/runtime/node.rs` | `pkg/runtime/shared_listener.rs`（拟建）、`pkg/runtime/shared_listener_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-087；未运行 |
| 88 实现成员加入预检 | `server/join/join.go`、`server/join/client_url.go`、`pkg/utils/memberutil/member.go` | `pkg/runtime/join_preflight.rs`（拟建）、`pkg/runtime/join_preflight_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-088；未运行 |
| 89 实现加入失败重试与恢复 | `server/join/join.go`、`pkg/member/`、`pkg/runtime/node.rs` | `pkg/runtime/join_resume.rs`（拟建）、`pkg/runtime/join_resume_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-089；未运行 |
| 90 实现配置文件与CLI合并 | `cmd/pd-server/main.go`、`cmd/pd-server/main.rs`、`server/config/` | `cmd/pd-server/cli_config.rs`（拟建）、`cmd/pd-server/cli_config_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-090；未运行 |
| 91 补齐帮助版本与退出码 | `cmd/pd-server/main.go`、`cmd/pd-server/main.rs` | `cmd/pd-server/cli_exit.rs`（拟建）、`cmd/pd-server/cli_exit_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-091；未运行 |
| 92 接入日志初始化与热级别 | `pkg/utils/logutil/`、`server/api/log.go`、`cmd/pd-server/main.rs` | `cmd/pd-server/cli_logging.rs`（拟建）、`cmd/pd-server/cli_logging_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-092；未运行 |
| 93 收敛启动失败清理 | `pkg/runtime/node.rs`、`pkg/basicserver/basic_server.go` | `pkg/runtime/start_cleanup.rs`（拟建）、`pkg/runtime/start_cleanup_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-093；未运行 |
| 94 收敛Ready状态门 | `server/server.go`、`server/apiv2/handlers/ready.go`、`pkg/runtime/node.rs` | `pkg/runtime/ready_gate.rs`（拟建）、`pkg/runtime/ready_gate_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-094；未运行 |
| 95 关闭长流与后台worker | `pkg/runtime/node.rs`、`pkg/schedule/`、`pkg/syncer/` | `pkg/runtime/shutdown_workers.rs`（拟建）、`pkg/runtime/shutdown_workers_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-095；未运行 |
| 96 验证宿主嵌入隔离 | `docs/rust/architecture.md`、`pkg/runtime/node_test.rs` | `pkg/runtime/embedded_isolation.rs`（拟建）、`pkg/runtime/embedded_isolation_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-096；未运行 |
| 97 验证独立进程信号关闭 | `cmd/pd-server/main_test.rs`、`cmd/pd-server/main.rs` | `cmd/pd-server/process_shutdown.rs`（拟建）、`cmd/pd-server/process_shutdown_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-097；未运行 |
| 98 验证多成员滚动重启 | `tests/rust-compat/harness.rs`、`pkg/member/`、`pkg/runtime/node.rs` | `tests/rust-compat/rolling_restart.rs`（拟建）、`tests/rust-compat/rolling_restart_test.rs`（拟建） | 依赖1–4及前序真实服务能力；真实listener/宿主/进程及失败清理 | 待补task-098；未运行 |
| 99 验证Go客户端核心业务 | `client/`、`tests/server/`、`tests/rust-compat/harness.rs` | `tests/rust-compat/go_client_acceptance.rs`（拟建）、`tests/rust-compat/go_client_acceptance_test.rs`（拟建） | 依赖1–4及前序真实服务能力；完整真实客户端/协议/HTTP/工作区回归 | 待补task-099；未运行 |
| 100 回归PD协议完整性 | `server/grpc_service.go`、`pkg/proto/lib.rs`、`tests/rust-compat/` | `tests/rust-compat/grpc_regression.rs`（拟建）、`tests/rust-compat/grpc_regression_test.rs`（拟建） | 依赖1–4及前序真实服务能力；完整真实客户端/协议/HTTP/工作区回归 | 待补task-100；未运行 |
| 101 回归HTTP与指标完整性 | `server/api/router.go`、`server/apiv2/router.go`、`tests/rust-compat/` | `tests/rust-compat/http_regression.rs`（拟建）、`tests/rust-compat/http_regression_test.rs`（拟建） | 依赖1–4及前序真实服务能力；完整真实客户端/协议/HTTP/工作区回归 | 待补task-101；未运行 |
| 102 完成工作区与来源覆盖审计 | `Cargo.toml`、`docs/rust/runtime.md` | `tests/rust-compat/workspace_acceptance.rs`（拟建）、`tests/rust-compat/workspace_acceptance_test.rs`（拟建） | 依赖1–4及前序真实服务能力；完整真实客户端/协议/HTTP/工作区回归 | 待补task-102；未运行 |

## 生产文件与公开声明索引

以下逐文件列出直接服务来源的Go导出函数/类型以及Rust pub声明（含pub(crate)）。owner来自上表上下文目录展开；精确RPC、HTTP路径、配置/指标归属由2/3细化。依赖目录的所有文件另在哈希表逐项固定。

| 文件 | owner | 公开符号（行号:声明） | 直接依赖定位 |
| --- | --- | --- | --- |
| [`cmd/pd-server/bin_main.rs`](../../../cmd/pd-server/bin_main.rs) | 102 | 无匹配导出声明；内部实现/注册仍纳入来源 | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`cmd/pd-server/lib.rs`](../../../cmd/pd-server/lib.rs) | 102 | 5:pub use entry::main; | pub use entry::main; |
| [`cmd/pd-server/main.go`](../../../cmd/pd-server/main.go) | 3,90,91 | 77:NewServiceCommand; 91:NewTSOServiceCommand; 112:NewSchedulingServiceCommand; 133:NewResourceManagerServiceCommand; 154:NewRouterServiceCommand; 175:NewPDServiceCommand | github.com/tikv/pd/pkg/dashboard; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/resourcemanager/redirector; github.com/tikv/pd/pkg/mcs/resourcemanager/server; github.com/tikv/pd/pkg/mcs/router/server; github.com/tikv/pd/pkg/mcs/scheduling/server; github.com/tikv/pd/pkg/mcs/tso/server; github.com/tikv/pd/pkg/memory; github.com/tikv/pd/pkg/schedule/schedulers; github.com/tikv/pd/pkg/swaggerserver; github.com/tikv/pd/pkg/utils/configutil; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/metricutil; github.com/tikv/pd/pkg/versioninfo; github.com/tikv/pd/server; github.com/tikv/pd/server/api; github.com/tikv/pd/server/apiv2; github.com/tikv/pd/server/config; github.com/tikv/pd/server/join |
| [`cmd/pd-server/main.rs`](../../../cmd/pd-server/main.rs) | 90,91,92,97 | 8:pub(crate) struct Config {; 43:pub(crate) async fn run_until(; 88:pub fn main() -> ExitCode { | use astersql_pd_runtime::PdNodeBuilder;; use astersql_pd_storage::{EtcdMetaStore, MetaStore};; use std::{error::Error, future::Future, process::ExitCode, sync::Arc, time::Duration};; use tokio_util::sync::CancellationToken; |
| [`pkg/basicserver/basic_server.go`](../../../pkg/basicserver/basic_server.go) | 3,43,93 | 25:Server | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/basicserver/metrics.go`](../../../pkg/basicserver/metrics.go) | 3,43,86 | 无匹配导出声明；内部实现/注册仍纳入来源 | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/proto/build.rs`](../../../pkg/proto/build.rs) | 2,4 | 无匹配导出声明；内部实现/注册仍纳入来源 | use std::{env, fs, path::PathBuf, process::Command};; use sha2::{Digest, Sha256}; |
| [`pkg/proto/lib.rs`](../../../pkg/proto/lib.rs) | 2,37,100 | 9:pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/kvproto.bin"));; 11:pub const SOURCE_COMMIT: &str = "f8ad079c8e9812ed5900b0c12b5f1d97171464c4"; | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/runtime/lib.rs`](../../../pkg/runtime/lib.rs) | 3,85 | 4:pub use node::{NodeConfig, NodeError, PdNode, PdNodeBuilder, bootstrap_config}; | pub use node::{NodeConfig, NodeError, PdNode, PdNodeBuilder, bootstrap_config}; |
| [`pkg/runtime/node.rs`](../../../pkg/runtime/node.rs) | 1,3,5,7,83,84,85,86,87,89,93,94,95,98 | 12:pub type NodeConfig = Identity;; 14:pub async fn bootstrap_config(; 59:pub type NodeError = Box<dyn std::error::Error + Send + Sync>;; 94:pub struct PdNodeBuilder {; 99:pub fn new(runtime: Handle, cancellation: CancellationToken) -> Self {; 106:pub async fn start(; 242:pub struct PdNode {; 249:pub async fn ready(&mut self) -> Result<(), NodeError> {; 258:pub fn events(&self) -> Vec<&'static str> {; 263:pub async fn wait(&mut self) -> Result<(), NodeError> {; 271:pub async fn shutdown(&mut self) -> Result<(), NodeError> { | use astersql_pd_id::Allocator;; use astersql_pd_member::{Leadership, Member};; use astersql_pd_server::{Identity, PdServices};; use astersql_pd_storage::MetaStore;; use astersql_pd_tso::{Oracle, SystemClock, Timestamp};; use prost::Message;; use std::sync::{Arc, Mutex};; use tokio::{net::TcpListener, runtime::Handle, sync::oneshot, task::JoinHandle};; use tokio_util::sync::CancellationToken;; use astersql_pd_storage::{Compare, Consistency, Mutation, Read}; |
| [`pkg/server/bucket_heartbeat_adapter.rs`](../../../pkg/server/bucket_heartbeat_adapter.rs) | 3,10 | 10:pub trait BucketHeartbeatService: Send + Sync {; 19:pub fn bucket_from_proto(buckets: astersql_pd_proto::metapb::Buckets) -> BucketReport { | use astersql_pd_cluster::BucketRegionNotFound;; use astersql_pd_statistics_buckets::{BucketFlows, BucketReport}; |
| [`pkg/server/core_region_adapter.rs`](../../../pkg/server/core_region_adapter.rs) | 3,23 | 8:pub fn peer_from_proto(p: metapb::Peer) -> core::Peer {; 23:pub fn peer_to_proto(p: core::Peer) -> metapb::Peer {; 38:pub fn region_from_proto(value: Option<metapb::Region>) -> Option<core::RegionMeta> {; 59:pub fn region_to_proto(p: core::RegionMeta) -> metapb::Region {; 82:pub fn region_from_heartbeat( | use astersql_pd_core as core;; use astersql_pd_proto::{metapb, pdpb}; |
| [`pkg/server/core_store_adapter.rs`](../../../pkg/server/core_store_adapter.rs) | 3,22 | 12:pub fn store_from_proto(value: Option<metapb::Store>) -> Option<core::StoreMeta> {; 16:pub fn store_to_proto(value: core::StoreMeta) -> metapb::Store {; 20:pub fn store_stats_from_proto(value: Option<pdpb::StoreStats>) -> Option<core::StoreStats> {; 24:pub fn store_stats_to_proto(value: core::StoreStats) -> pdpb::StoreStats { | use astersql_pd_core as core;; use astersql_pd_proto::{apipb, metapb, pdpb}; |
| [`pkg/server/core_trace.rs`](../../../pkg/server/core_trace.rs) | 3 | 9:pub enum Stage {; 21:pub const ALL: [Self; 9] = [; 32:pub fn label(self) -> &'static str {; 48:pub struct HeartbeatMetrics {; 52:pub fn register(registry: &Registry) -> Result<Self, prometheus::Error> {; 80:pub fn duration(&self, stage: Stage) -> f64 {; 83:pub fn count(&self, stage: Stage) -> f64 {; 91:pub struct HeartbeatTracer<'a, C> {; 97:pub fn new(metrics: &'a HeartbeatMetrics, clock: C) -> Self {; 104:pub fn begin(&mut self) {; 107:pub fn reset_checkpoint(&mut self) {; 110:pub fn on_save_cache_begin(&mut self) {; 113:pub fn on_save_cache_finished(&mut self) {; 116:pub fn finish(&mut self, stage: Stage) { | use prometheus::{Counter, CounterVec, Opts, Registry};; use std::time::Duration; |
| [`pkg/server/examples/bucket_heartbeat_oracle.rs`](../../../pkg/server/examples/bucket_heartbeat_oracle.rs) | 3 | 无匹配导出声明；内部实现/注册仍纳入来源 | use astersql_pd_proto::{ |
| [`pkg/server/examples/region_heartbeat_oracle.rs`](../../../pkg/server/examples/region_heartbeat_oracle.rs) | 3 | 无匹配导出声明；内部实现/注册仍纳入来源 | use astersql_pd_proto::{ |
| [`pkg/server/examples/store_heartbeat_oracle.rs`](../../../pkg/server/examples/store_heartbeat_oracle.rs) | 3 | 无匹配导出声明；内部实现/注册仍纳入来源 | use astersql_pd_proto::{ |
| [`pkg/server/examples/sync_codec_oracle.rs`](../../../pkg/server/examples/sync_codec_oracle.rs) | 3 | 无匹配导出声明；内部实现/注册仍纳入来源 | use astersql_pd_proto::{; use prost::{Message, bytes::Buf}; |
| [`pkg/server/grpc.rs`](../../../pkg/server/grpc.rs) | 3,18,25,36 | 8:pub struct Grpc {; 30:pub(crate) fn member(id: &Identity) -> pdpb::Member { | use crate::*;; use astersql_pd_proto::pdpb::{self, pd_server::Pd};; use std::pin::Pin;; use tokio_stream::Stream;; use tonic::{Request, Response, Status};; use crate::store_heartbeat_adapter::StoreHeartbeatError; |
| [`pkg/server/http.rs`](../../../pkg/server/http.rs) | 3,38,87 | 31:pub fn router(services: Services) -> Router { | use crate::*;; use axum::{Json, Router, extract::State, http::StatusCode, routing::get};; use serde_json::{Value, json}; |
| [`pkg/server/lib.rs`](../../../pkg/server/lib.rs) | 1,3,5,87 | 12:pub struct Identity {; 21:pub trait PdServices: Send + Sync {; 50:pub fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> axum::Router {; 61:pub mod core_trace;; 66:pub mod core_store_adapter;; 71:pub mod core_region_adapter;; 76:pub mod store_heartbeat_adapter;; 81:pub mod region_heartbeat_adapter;; 86:pub mod bucket_heartbeat_adapter;; 91:pub mod sync_codec;; 96:pub mod sync_transport;; 101:pub mod schedule_wire;; 106:pub mod schedule_admin_balance_leader;; 111:pub mod schedule_admin_balance_region;; 116:pub mod schedule_admin_balance_witness;; 121:pub mod schedule_admin_evict_leader;; 126:pub mod schedule_admin_evict_slow_store;; 131:pub mod schedule_admin_evict_stopping_store;; 136:pub mod schedule_admin_grant_hot_region;; 141:pub mod schedule_admin_hot_region;; 146:pub mod schedule_admin_grant_leader;; 152:pub mod schedule_admin_label;; 157:pub mod schedule_admin_random_merge;; 162:pub mod schedule_admin_scatter_range;; 167:pub mod schedule_admin_shuffle_hot_region;; 172:pub mod schedule_admin_shuffle_leader;; 177:pub mod schedule_admin_shuffle_region;; 182:pub mod schedule_admin_transfer_witness_leader;; 187:pub mod schedule_admin_evict_slow_trend;; 192:pub mod schedule_admin_balance_range; | use astersql_pd_tso::Timestamp;; use async_trait::async_trait;; use std::sync::Arc;; use tokio_util::sync::CancellationToken; |
| [`pkg/server/region_heartbeat_adapter.rs`](../../../pkg/server/region_heartbeat_adapter.rs) | 3,9 | 7:pub struct RegionHeartbeatError(pub String);; 12:pub trait RegionHeartbeatService: Send + Sync { | use astersql_pd_core::{NodeState, RegionInfo}; |
| [`pkg/server/schedule_admin_balance_leader.rs`](../../../pkg/server/schedule_admin_balance_leader.rs) | 3,65 | 34:pub async fn serve(; 136:pub(crate) fn json_type(value: &Value) -> &'static str {; 146:pub(crate) fn syntax_error(bytes: &[u8], error: &serde_json::Error) -> String {; 155:pub(crate) fn decode_ranges(value: &Value) -> Result<Vec<KeyRange>, String> { | use astersql_pd_core::KeyRange;; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::{BalanceLeaderScheduler, LeaderConfigUpdate};; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::Value; |
| [`pkg/server/schedule_admin_balance_range.rs`](../../../pkg/server/schedule_admin_balance_range.rs) | 3,65 | 36:pub async fn serve(jobs: &mut BalanceRangeJobs, now: JobTime, request: Request<Body>) -> Response { | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_schedule_schedulers::{BalanceRangeJob, BalanceRangeJobs, JobTime, get_key_ranges};; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_balance_region.rs`](../../../pkg/server/schedule_admin_balance_region.rs) | 3 | 11:pub fn serve(_scheduler: &BalanceRegionScheduler, _request: Request<Body>) -> Response { | use astersql_pd_schedule_schedulers::BalanceRegionScheduler;; use axum::{body::Body, http::Request, response::Response}; |
| [`pkg/server/schedule_admin_balance_witness.rs`](../../../pkg/server/schedule_admin_balance_witness.rs) | 3 | 63:pub async fn serve( | use crate::schedule_admin_balance_leader::{decode_ranges, json_type, syntax_error};; use astersql_pd_core::KeyRange;; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::{BalanceWitnessConfig, BalanceWitnessScheduler};; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde::{; use serde_json::Value; |
| [`pkg/server/schedule_admin_evict_leader.rs`](../../../pkg/server/schedule_admin_evict_leader.rs) | 3 | 54:pub async fn serve( | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_core::{BasicCluster, StoreControlError};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::{EvictConfigError, EvictLeaderScheduler, EvictRemoveError};; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_evict_slow_store.rs`](../../../pkg/server/schedule_admin_evict_slow_store.rs) | 3 | 26:pub async fn serve( | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::{EvictDiskScheduler, EvictNetworkScheduler};; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_evict_slow_trend.rs`](../../../pkg/server/schedule_admin_evict_slow_trend.rs) | 3 | 35:pub async fn serve( | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::SlowTrendScheduler;; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_evict_stopping_store.rs`](../../../pkg/server/schedule_admin_evict_stopping_store.rs) | 3 | 26:pub async fn serve( | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::EvictStoppingScheduler;; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_grant_hot_region.rs`](../../../pkg/server/schedule_admin_grant_hot_region.rs) | 3 | 26:pub async fn serve( | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::GrantHotScheduler;; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_grant_leader.rs`](../../../pkg/server/schedule_admin_grant_leader.rs) | 3 | 35:pub async fn serve( | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_core::{BasicCluster, StoreControlError};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::{GrantConfigError, GrantLeaderScheduler, GrantRemoveError};; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_hot_region.rs`](../../../pkg/server/schedule_admin_hot_region.rs) | 3 | 63:pub async fn serve( | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::{HotConfigUpdate, HotRegionConfig, HotRegionParams};; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde::de::{MapAccess, Visitor};; use serde::{Deserialize, Deserializer};; use serde_json::Value; |
| [`pkg/server/schedule_admin_label.rs`](../../../pkg/server/schedule_admin_label.rs) | 3 | 11:pub fn serve(_scheduler: &LabelScheduler, _request: Request<Body>) -> Response { | use astersql_pd_schedule_schedulers::LabelScheduler;; use axum::{body::Body, http::Request, response::Response}; |
| [`pkg/server/schedule_admin_random_merge.rs`](../../../pkg/server/schedule_admin_random_merge.rs) | 3 | 11:pub fn serve(_scheduler: &RandomMergeScheduler, _request: Request<Body>) -> Response { | use astersql_pd_schedule_schedulers::RandomMergeScheduler;; use axum::{body::Body, http::Request, response::Response}; |
| [`pkg/server/schedule_admin_scatter_range.rs`](../../../pkg/server/schedule_admin_scatter_range.rs) | 3 | 35:pub async fn serve( | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::{ScatterRangeError, ScatterRangeScheduler};; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_shuffle_hot_region.rs`](../../../pkg/server/schedule_admin_shuffle_hot_region.rs) | 3 | 35:pub async fn serve( | use crate::schedule_admin_balance_leader::{json_type, syntax_error};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::{ShuffleHotConfig, ShuffleHotScheduler};; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_shuffle_leader.rs`](../../../pkg/server/schedule_admin_shuffle_leader.rs) | 3 | 11:pub fn serve(_scheduler: &ShuffleLeaderScheduler, _request: Request<Body>) -> Response { | use astersql_pd_schedule_schedulers::ShuffleLeaderScheduler;; use axum::{body::Body, http::Request, response::Response}; |
| [`pkg/server/schedule_admin_shuffle_region.rs`](../../../pkg/server/schedule_admin_shuffle_region.rs) | 3 | 64:pub async fn serve( | use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_schedulers::{ShuffleRegionError, ShuffleRegionScheduler};; use astersql_pd_storage_endpoint::StorageEndpoint;; use axum::{; use futures_util::StreamExt;; use serde_json::{Value, json}; |
| [`pkg/server/schedule_admin_transfer_witness_leader.rs`](../../../pkg/server/schedule_admin_transfer_witness_leader.rs) | 3 | 11:pub fn serve(_scheduler: &TransferWitnessLeaderScheduler, _request: Request<Body>) -> Response { | use astersql_pd_schedule_schedulers::TransferWitnessLeaderScheduler;; use axum::{body::Body, http::Request, response::Response}; |
| [`pkg/server/schedule_wire.rs`](../../../pkg/server/schedule_wire.rs) | 3 | 10:pub struct WireError {; 39:pub fn to_pdpb(c: &HeartbeatCommand) -> Result<pdpb::RegionHeartbeatResponse, WireError> {; 103:pub fn to_scheduling(; 126:pub fn error_pdpb(; 145:pub fn keepalive_pdpb(cluster_id: u64) -> pdpb::RegionHeartbeatResponse {; 155:pub fn keepalive_scheduling(cluster_id: u64) -> schedulingpb::RegionHeartbeatResponse { | use crate::core_region_adapter::{peer_to_proto, region_to_proto};; use astersql_pd_core::Peer;; use astersql_pd_proto::{metapb, pdpb, schedulingpb};; use astersql_pd_schedule_hbstream::{self as domain, HeartbeatCommand}; |
| [`pkg/server/store_heartbeat_adapter.rs`](../../../pkg/server/store_heartbeat_adapter.rs) | 3,8 | 8:pub enum StoreHeartbeatError {; 18:pub trait StoreHeartbeatService: Send + Sync {; 24:pub fn apply_store_heartbeat( | use astersql_pd_core::{BasicCluster, NodeState, StoreStats};; use astersql_pd_statistics::HotStat; |
| [`pkg/server/sync_codec.rs`](../../../pkg/server/sync_codec.rs) | 3 | 18:pub struct SyncRequestHeader {; 47:pub struct SyncMember {; 91:pub struct SyncRequest {; 97:pub fn request_from_proto(p: pdpb::SyncRegionRequest) -> SyncRequest {; 105:pub fn request_to_proto(p: SyncRequest) -> pdpb::SyncRegionRequest {; 114:pub struct SyncHeader {; 120:pub struct SyncBuckets {; 127:pub struct SyncDownPeer {; 134:pub struct SyncResponse {; 148:pub fn into_apply(self) -> SyncApplyResponse {; 169:pub fn response_from_proto(p: pdpb::SyncRegionResponse) -> SyncResponse {; 237:pub fn response_to_proto(p: SyncResponse) -> pdpb::SyncRegionResponse {; 345:pub fn history_to_proto(cluster_id: u64, frame: &SyncFrame) -> pdpb::SyncRegionResponse {; 349:pub fn snapshot_to_proto(cluster_id: u64, batch: SnapshotBatch<'_>) -> pdpb::SyncRegionResponse {; 353:pub fn catch_up_to_proto(cluster_id: u64, frame: CatchUpFrame<'_>) -> pdpb::SyncRegionResponse { | use crate::core_region_adapter::{; use astersql_pd_core::{Peer, RegionBucketMeta, RegionBuckets, RegionInfo, RegionMeta};; use astersql_pd_proto::{metapb, pdpb};; use astersql_pd_statistics_buckets::BucketFlows;; use astersql_pd_syncer::{; use std::sync::Arc; |
| [`pkg/server/sync_transport.rs`](../../../pkg/server/sync_transport.rs) | 3,11 | 24:pub struct RegionSyncer {; 47:pub fn new(; 69:pub fn for_heartbeat_state<C: Send + 'static>(; 89:pub fn downstreams(&self) -> Downstreams {; 93:pub fn publish(&self, region: Arc<RegionInfo>) -> Result<(), String> {; 108:pub fn downstream_names(&self) -> Vec<String> {; 114:pub async fn stop(&self) {; 246:pub(crate) struct WireSender {; 267:pub(crate) struct SyncGrpc(pub Grpc);; 346:pub struct SyncFollowerState {; 356:pub struct SyncFollower { | use crate::{grpc::Grpc, sync_codec::*};; use astersql_pd_core::{BasicCluster, RegionInfo};; use astersql_pd_proto::pdpb::{self, pd_server::Pd};; use astersql_pd_syncer::{Downstreams, HistoryBuffer, SyncFrame};; use std::{; use tokio::{runtime::Handle, sync::mpsc, task::JoinHandle};; use tokio_stream::{Stream, wrappers::ReceiverStream};; use tokio_util::sync::CancellationToken;; use tonic::{Request, Response, Status}; |
| [`pkg/tso/admin.go`](../../../pkg/tso/admin.go) | 35,36,75,76 | 29:Handler; 34:AdminHandler; 40:NewAdminHandler; 69:ResetTS | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/utils/apiutil |
| [`pkg/tso/allocator.go`](../../../pkg/tso/allocator.go) | 35,36,75,76 | 53:Allocator; 76:NewAllocator; 148:Close; 156:Initialize; 168:IsInitialize; 173:UpdateTSO; 215:SetTSO; 220:GenerateTSO; 232:Reset; 420:GetPrimaryAddr; 432:GetMember | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/utils; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/member; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/pkg/utils/keypath; github.com/tikv/pd/pkg/utils/logutil |
| [`pkg/tso/config.go`](../../../pkg/tso/config.go) | 35,36,75,76 | 24:ServiceConfig; 38:Config | github.com/tikv/pd/pkg/utils/grpcutil |
| [`pkg/tso/keyspace_group_manager.go`](../../../pkg/tso/keyspace_group_manager.go) | 35,36,75,76 | 165:SetModRevision; 359:KeyspaceGroupManager; 409:NewKeyspaceGroupManager; 445:Initialize; 467:Close; 483:GetServiceConfig; 491:InitializeTSOServerWatchLoop; 540:InitializeGroupWatchLoop; 1072:GetAllocator; 1083:FindGroupByKeyspaceID; 1095:GetMember; 1109:GetKeyspaceGroups; 1124:GetServingKeyspaceGroups; 1139:GetKeyspaceGroupByID; 1151:HandleTSORequest; 1194:GetMinTS | github.com/tikv/pd/pkg/election; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/keyspace/constant; github.com/tikv/pd/pkg/mcs/discovery; github.com/tikv/pd/pkg/mcs/utils; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/member; github.com/tikv/pd/pkg/slice; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/pkg/storage/kv; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/etcdutil; github.com/tikv/pd/pkg/utils/keypath; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/memberutil; github.com/tikv/pd/pkg/utils/syncutil; github.com/tikv/pd/pkg/utils/tsoutil; github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/pkg/versioninfo/kerneltype |
| [`pkg/tso/lib.rs`](../../../pkg/tso/lib.rs) | 35,36,75,76 | 4:pub use oracle::*; | pub use oracle::*; |
| [`pkg/tso/metrics.go`](../../../pkg/tso/metrics.go) | 35,36,75,76 | 231:DeleteKeyspaceListLengthMetric | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/tso/oracle.rs`](../../../pkg/tso/oracle.rs) | 35,36,75,76 | 12:pub const MAX_LOGICAL: u32 = 1 << 18;; 19:pub struct Timestamp {; 25:pub enum TsoError {; 49:pub trait Clock: Send + Sync {; 53:pub struct SystemClock;; 73:pub struct Oracle {; 84:pub async fn new(; 200:pub async fn allocate(&self, count: u32) -> Result<Timestamp, TsoError> {; 236:pub async fn update(&self) -> Result<(), TsoError> { | use astersql_pd_member::{Leadership, MemberError};; use astersql_pd_storage_kv::{Compare, Consistency, MetaStore, Mutation, Read, StoreError};; use std::{; use tokio::sync::Mutex;; use tokio_util::sync::CancellationToken; |
| [`pkg/tso/testutil.go`](../../../pkg/tso/testutil.go) | 35,36,75,76 | 26:TestServiceConfig; 39:GetName; 44:GetBackendEndpoints; 49:GetListenAddr; 54:GetAdvertiseListenAddr; 59:GetLease; 64:GetTSOUpdatePhysicalInterval; 69:GetTSOSaveInterval; 74:GetMaxResetTSGap; 79:GetTLSConfig | github.com/tikv/pd/pkg/utils/grpcutil |
| [`pkg/tso/tso.go`](../../../pkg/tso/tso.go) | 35,36,75,76 | 无匹配导出声明；内部实现/注册仍纳入来源 | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/member; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/syncutil; github.com/tikv/pd/pkg/utils/tsoutil; github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/server/config |
| [`pkg/tso/util.go`](../../../pkg/tso/util.go) | 35,36,75,76 | 25:ExtractKeyspaceGroupIDFromPath | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/apiutil/apiutil.go`](../../../pkg/utils/apiutil/apiutil.go) | 38,41,82 | 92:DeferClose; 99:JSONError; 103:Error; 108:TagJSONError; 122:ErrorResp; 140:GetIPPortFromHTTPRequest; 172:GetCallerIDOnHTTP; 182:CallerIDRoundTripper; 188:NewCallerIDRoundTripper; 196:RoundTrip; 204:GetRouteName; 213:AccessPath; 219:NewAccessPath; 224:PostJSON; 234:GetJSON; 243:GetJSONWithoutBody; 252:PatchJSON; 262:PutJSON; 272:PostJSONIgnoreResp; 278:DoDelete; 287:DoDeleteWithContext; 311:FieldError; 319:ParseUint64VarsField; 332:CollectEscapeStringOption; 347:CollectStringOption; 363:ParseKey; 385:ParseHexKeys; 405:ReadJSON; 423:ReadJSONRespondError; 446:APIServiceGroup; 454:Path; 468:RegisterUserDefinedHandlers; 487:NewCustomReverseProxies; 497:ServeHTTP; 572:ParseTime; 584:IsPathInDirectory | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/slice |
| [`pkg/utils/apiutil/multiservicesapi/middleware.go`](../../../pkg/utils/apiutil/multiservicesapi/middleware.go) | 41,82 | 41:ServiceRedirector | github.com/tikv/pd/pkg/basicserver; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/utils/apiutil |
| [`pkg/utils/apiutil/serverapi/middleware.go`](../../../pkg/utils/apiutil/serverapi/middleware.go) | 40,41,82 | 43:NewRuntimeServiceValidator; 47:ServeHTTP; 56:IsServiceAllowed; 82:RedirectRule; 91:NewRedirector; 100:RedirectorOption; 103:MicroserviceRedirectRule; 130:MatchMicroserviceRedirect; 199:ServeHTTP | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/slice; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`pkg/utils/grpcutil/cluster.go`](../../../pkg/utils/grpcutil/cluster.go) | 42,82,83,84,85 | 35:GetRegion; 64:GetPrevRegion; 91:GetRegionByID; 118:ScanRegions; 146:BatchScanRegions; 208:QueryRegion; 227:GetStore; 248:GetAllStores; 269:WrapHeader; 278:WrapErrorToHeader; 286:ErrorHeader; 294:NotBootstrappedHeader; 302:RegionNotFound | github.com/tikv/pd/pkg/core; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/utils/keypath; github.com/tikv/pd/pkg/utils/keyutil |
| [`pkg/utils/grpcutil/grpcutil.go`](../../../pkg/utils/grpcutil/grpcutil.go) | 42,82,83,84,85 | 50:TLSConfig; 66:ToTLSInfo; 80:ToClientTLSConfig; 116:ToServerTLSConfig; 151:GetClientConn; 183:BuildForwardContext; 189:ResetForwardContext; 199:GetForwardedHost; 208:IsFollowerHandleEnabled; 236:CreateClientConn; 259:CheckStream; 274:NeedRebuildConnection | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/utils/logutil |
| [`pkg/utils/grpcutil/lib.rs`](../../../pkg/utils/grpcutil/lib.rs) | 42,82,83,84,85 | 7:pub struct TLSConfig {; 30:pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {; 36:pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> { | use serde::{Deserialize, Serialize};; use base64::{Engine, engine::general_purpose::STANDARD};; use serde::{Deserialize, Deserializer, Serializer}; |
| [`pkg/utils/grpcutil/metrics.go`](../../../pkg/utils/grpcutil/metrics.go) | 42,82,83,84,85 | 35:RequestCounter | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/keypath/absolute_key_path.go`](../../../pkg/utils/keypath/absolute_key_path.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 143:MsParam; 149:Prefix; 154:AllocIDPath; 159:KeyspaceAllocIDPath; 164:RegistryPath; 169:ServicePath; 174:ClusterPath; 179:ClusterBootstrapTimePath; 184:StorePath; 189:StorePathPrefix; 194:ExtractStoreIDFromPath; 200:MinResolvedTSPath; 205:ExternalTimestampPath; 210:RecoveringMarkPath; 215:PitrRestoreModeMarkPath; 220:KeyspaceMetaPrefix; 225:KeyspaceMetaPath; 230:KeyspaceIDPath; 235:KeyspaceGroupIDPrefix; 240:KeyspaceGroupIDPath; 245:GetCompiledKeyspaceGroupIDRegexp; 250:ServiceMiddlewarePath; 255:StoreLeaderWeightPath; 260:StoreRegionWeightPath; 265:ReplicationModePath; 270:TimestampPath; 278:RegionPath; 313:MetaServiceGroupStatusPath | github.com/tikv/pd/pkg/keyspace/constant |
| [`pkg/utils/keypath/affinity.go`](../../../pkg/utils/keypath/affinity.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 23:AffinityGroupPath; 30:AffinityGroupsPrefix | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/keypath/cluster_id.go`](../../../pkg/utils/keypath/cluster_id.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 24:ClusterID; 33:SetClusterID; 38:ResetClusterID | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/keypath/cluster_paths.rs`](../../../pkg/utils/keypath/cluster_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | 5:pub const CLUSTER_ID_PATH: &str = "/pd/cluster_id";; 9:pub struct ClusterPaths {; 15:pub enum StoreIdError {; 34:pub const fn new(cluster_id: u64) -> Self {; 38:pub const fn cluster_id(&self) -> u64 {; 42:pub fn cluster_path(&self) -> String {; 46:pub fn cluster_bootstrap_time_path(&self) -> String {; 50:pub fn alloc_id_path(&self) -> String {; 54:pub fn keyspace_alloc_id_path(&self) -> String {; 58:pub fn store_path_prefix(&self) -> String {; 62:pub fn store_path(&self, store_id: u64) -> String {; 66:pub fn region_path(&self, region_id: u64) -> String {; 71:pub fn extract_store_id_from_path(&self, path: &str) -> Result<u64, StoreIdError> { | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/keypath/config.go`](../../../pkg/utils/keypath/config.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 20:ConfigPath; 25:SchedulerConfigPathPrefix; 30:SchedulerConfigPath | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/keypath/election.go`](../../../pkg/utils/keypath/election.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 24:ElectionPath; 38:ExpectedPrimaryPath; 49:MemberLeaderPriorityPath | github.com/tikv/pd/pkg/mcs/utils/constant |
| [`pkg/utils/keypath/gc_paths.rs`](../../../pkg/utils/keypath/gc_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | 6:pub const NULL_KEYSPACE_ID: u32 = u32::MAX;; 8:pub const GC_WORKER_SERVICE_SAFE_POINT_ID: &str = "gc_worker";; 10:pub const NATIVE_BR_SERVICE_SAFE_POINT_ID: &str = "native_br";; 14:pub fn gc_state_revision_path(&self) -> String {; 18:pub fn gc_safe_point_path(&self, keyspace_id: u32) -> String {; 26:pub fn txn_safe_point_path(&self, keyspace_id: u32) -> String {; 34:pub fn gc_barrier_prefix(&self, keyspace_id: u32) -> String {; 38:pub fn gc_barrier_path(&self, keyspace_id: u32, barrier_id: &str) -> String {; 49:pub fn global_gc_barrier_prefix(&self) -> String {; 53:pub fn global_gc_barrier_path(&self, barrier_id: &str) -> String {; 60:pub fn service_gc_safe_point_prefix(&self) -> String {; 64:pub fn service_gc_safe_point_path(&self, service_id: &str) -> String {; 68:pub fn compatible_tidb_min_start_ts_prefix(&self, keyspace_id: u32) -> String {; 76:pub fn gc_safe_point_v2_prefix(&self) -> String {; 80:pub fn gc_safe_point_v2_path(&self, keyspace_id: u32) -> String {; 84:pub fn service_safe_point_v2_path(&self, keyspace_id: u32, service_id: &str) -> String {; 91:pub fn service_safe_point_v2_prefix(&self, keyspace_id: u32) -> String { | use crate::ClusterPaths; |
| [`pkg/utils/keypath/gc_states.go`](../../../pkg/utils/keypath/gc_states.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 24:GCStateRevisionPath; 30:GCSafePointPath; 38:TxnSafePointPath; 46:GCBarrierPrefix; 51:GCBarrierPath; 59:GlobalGCBarrierPrefix; 64:GlobalGCBarrierPath; 70:ServiceGCSafePointPrefix; 76:ServiceGCSafePointPath; 81:CompatibleTiDBMinStartTSPrefix; 89:GCSafePointV2Path; 94:ServiceSafePointV2Path; 100:ServiceSafePointV2Prefix; 105:GCSafePointV2Prefix | github.com/tikv/pd/pkg/keyspace/constant |
| [`pkg/utils/keypath/keyspace_time_paths.rs`](../../../pkg/utils/keypath/keyspace_time_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | 7:pub fn keyspace_meta_prefix(&self) -> String {; 11:pub fn keyspace_meta_path(&self, space_id: u32) -> String {; 15:pub fn keyspace_id_path(&self, name: &str) -> String {; 19:pub fn keyspace_group_id_prefix(&self) -> String {; 23:pub fn keyspace_group_id_path(&self, group_id: u32) -> String {; 27:pub fn timestamp_path(&self, group_id: u32) -> String {; 35:pub fn min_resolved_ts_path(&self) -> String {; 39:pub fn external_timestamp_path(&self) -> String {; 43:pub fn meta_service_group_status_path(&self, group_id: &str) -> String {; 54:pub fn keyspace_group_id_from_path(path: &str) -> Option<&str> { | use crate::ClusterPaths; |
| [`pkg/utils/keypath/lib.rs`](../../../pkg/utils/keypath/lib.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | 4:pub use cluster_paths::*;; 10:pub use member_config_paths::*;; 16:pub use gc_paths::*;; 22:pub use rule_resource_paths::*;; 28:pub use keyspace_time_paths::*; | pub use cluster_paths::*;; pub use member_config_paths::*;; pub use gc_paths::*;; pub use rule_resource_paths::*;; pub use keyspace_time_paths::*; |
| [`pkg/utils/keypath/maintenance.go`](../../../pkg/utils/keypath/maintenance.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 22:MaintenanceTaskPath; 27:MaintenanceTaskPathPrefix | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/keypath/member.go`](../../../pkg/utils/keypath/member.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 20:MemberBinaryDeployPath; 25:MemberGitHashPath; 30:MemberBinaryVersionPath | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/keypath/member_config_paths.rs`](../../../pkg/utils/keypath/member_config_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | 7:pub struct MsParam {; 15:pub fn prefix(path: &str) -> String {; 44:pub fn config_path(&self) -> String {; 48:pub fn scheduler_config_path_prefix(&self) -> String {; 52:pub fn scheduler_config_path(&self, name: &str) -> String {; 56:pub fn service_middleware_path(&self) -> String {; 60:pub fn replication_mode_path(&self, mode: &str) -> String {; 64:pub fn store_leader_weight_path(&self, id: u64) -> String {; 71:pub fn store_region_weight_path(&self, id: u64) -> String {; 78:pub fn member_binary_deploy_path(&self, id: u64) -> String {; 82:pub fn member_git_hash_path(&self, id: u64) -> String {; 86:pub fn member_binary_version_path(&self, id: u64) -> String {; 90:pub fn member_leader_priority_path(&self, id: u64) -> String {; 95:pub fn election_path(&self, param: Option<&MsParam>) -> String {; 119:pub fn expected_primary_path(&self, param: &MsParam) -> String { | use crate::ClusterPaths; |
| [`pkg/utils/keypath/registry_marker_paths.rs`](../../../pkg/utils/keypath/registry_marker_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | 7:pub fn service_path(&self, service_name: &str) -> String {; 11:pub fn registry_path(&self, service_name: &str, service_addr: &str) -> String {; 15:pub fn recovering_mark_path(&self) -> String {; 22:pub fn pitr_restore_mode_mark_path(&self) -> String {; 29:pub fn maintenance_task_path(&self, task_type: &str) -> String {; 33:pub fn maintenance_task_path_prefix(&self) -> String { | use crate::ClusterPaths; |
| [`pkg/utils/keypath/resource_group.go`](../../../pkg/utils/keypath/resource_group.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 26:ControllerConfigPath; 41:ResourceGroupSettingPrefix; 46:ResourceGroupStatePrefix; 51:ResourceGroupPrefix; 56:KeyspaceResourceGroupSettingPath; 64:KeyspaceResourceGroupStatePath; 72:KeyspaceResourceGroupSettingPrefix; 77:KeyspaceResourceGroupStatePrefix; 82:KeyspaceServiceLimitPath; 87:KeyspaceServiceLimitPrefix; 92:ParseKeyspaceResourceGroupPath | github.com/tikv/pd/pkg/keyspace/constant |
| [`pkg/utils/keypath/rule.go`](../../../pkg/utils/keypath/rule.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | 22:RulesPathPrefix; 27:RuleKeyPath; 32:RuleCommonPathPrefix; 37:RuleGroupIDPath; 42:RegionLabelKeyPath; 47:RuleGroupPathPrefix; 52:RegionLabelPathPrefix | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/keypath/rule_resource_paths.rs`](../../../pkg/utils/keypath/rule_resource_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | 7:pub fn rule_key_path(&self, name: &str) -> String {; 11:pub fn rule_group_id_path(&self, name: &str) -> String {; 15:pub fn region_label_key_path(&self, name: &str) -> String {; 19:pub fn affinity_group_path(&self, name: &str) -> String {; 23:pub fn rules_path_prefix(&self) -> String {; 27:pub fn rule_group_path_prefix(&self) -> String {; 31:pub fn region_label_path_prefix(&self) -> String {; 35:pub fn affinity_groups_prefix(&self) -> String {; 39:pub fn rule_common_path_prefix(&self) -> String {; 44:pub const fn controller_config_path() -> &'static str {; 48:pub const fn resource_group_prefix() -> &'static str {; 52:pub const fn resource_group_setting_prefix() -> &'static str {; 56:pub const fn resource_group_state_prefix() -> &'static str {; 60:pub const fn keyspace_resource_group_setting_prefix() -> &'static str {; 64:pub const fn keyspace_resource_group_state_prefix() -> &'static str {; 68:pub const fn keyspace_service_limit_prefix() -> &'static str {; 72:pub fn keyspace_resource_group_setting_path(id: u32, name: &str) -> String {; 80:pub fn keyspace_resource_group_state_path(id: u32, name: &str) -> String {; 88:pub fn keyspace_service_limit_path(id: u32) -> String {; 94:pub enum ResourceGroupPathError {; 113:pub fn parse_keyspace_resource_group_path( | use crate::{ClusterPaths, NULL_KEYSPACE_ID}; |
| [`pkg/utils/logutil/lib.rs`](../../../pkg/utils/logutil/lib.rs) | 92 | 7:pub struct LogConfig {; 41:pub struct FileLogConfig {; 59:pub struct SamplingConfig {; 69:pub fn adjust(&mut self, disable_error_verbose_defined: bool) {; 84:pub enum RedactInfoLog { | use serde::{Deserialize, Serialize}; |
| [`pkg/utils/logutil/log.go`](../../../pkg/utils/logutil/log.go) | 92 | 33:FileLogConfig; 45:LogConfig; 57:StringToZapLogLevel; 74:SetupLogger; 92:LogPanic; 99:RedactInfoLogType; 111:MarshalJSON; 125:UnmarshalJSON; 146:UnmarshalTOML; 206:ZapRedactByteString; 211:ZapRedactString; 216:ZapRedactStringer; 221:RedactBytes; 234:RedactString; 246:RedactStringer; 262:String; 269:CondUint32; 277:IsLevelLegal | github.com/tikv/pd/pkg/errs |
| [`pkg/utils/memberutil/member.go`](../../../pkg/utils/memberutil/member.go) | 88 | 24:GenerateUniqueID | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/metricutil/lib.rs`](../../../pkg/utils/metricutil/lib.rs) | 86 | 8:pub struct MetricConfig {; 21:pub fn adjust(&mut self, name: &str) { | use astersql_pd_typeutil::Duration;; use serde::{Deserialize, Serialize}; |
| [`pkg/utils/metricutil/metricutil.go`](../../../pkg/utils/metricutil/metricutil.go) | 86 | 40:MetricConfig; 96:Push; 117:EnablePyroscope | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/typeutil |
| [`pkg/utils/netutil/address.go`](../../../pkg/utils/netutil/address.go) | 39 | 34:ResolveLoopBackAddr; 47:IsEnableHTTPS | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/requestutil/context.go`](../../../pkg/utils/requestutil/context.go) | 40,81 | 32:WithRequestInfo; 37:RequestInfoFrom; 43:WithEndTime; 48:EndTimeFrom | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/requestutil/request_info.go`](../../../pkg/utils/requestutil/request_info.go) | 40,81 | 29:RequestInfo; 42:String; 49:GetRequestInfo; 57:GetRequestInfoWithoutBody; 72:CaptureBody | github.com/tikv/pd/pkg/utils/apiutil |
| [`pkg/utils/tempurl/check_env_dummy.go`](../../../pkg/utils/tempurl/check_env_dummy.go) | 39 | 无匹配导出声明；内部实现/注册仍纳入来源 | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`pkg/utils/tempurl/check_env_linux.go`](../../../pkg/utils/tempurl/check_env_linux.go) | 39 | 无匹配导出声明；内部实现/注册仍纳入来源 | github.com/tikv/pd/pkg/errs |
| [`pkg/utils/tempurl/tempurl.go`](../../../pkg/utils/tempurl/tempurl.go) | 39 | 49:Alloc | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/utils/syncutil |
| [`server/api/admin.go`](../../../server/api/admin.go) | 73,74,75,76 | 43:RecoveryStatusResponse; 64:DeleteRegionCache; 101:DeleteRegionStorage; 146:DeleteAllRegionCache; 175:SavePersistFile | github.com/tikv/pd/pkg/core; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`server/api/checker.go`](../../../server/api/checker.go) | 66 | 52:PauseOrResumeChecker; 89:GetCheckerStatus | github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`server/api/cluster.go`](../../../server/api/cluster.go) | 46 | 44:GetCluster; 56:GetClusterStatus | github.com/tikv/pd/server |
| [`server/api/config.go`](../../../server/api/config.go) | 47,48,49,50 | 72:GetConfig; 110:GetDefaultConfig; 133:SetConfig; 425:GetScheduleConfig; 453:SetScheduleConfig; 495:GetReplicationConfig; 525:SetReplicationConfig; 545:GetLabelPropertyConfig; 560:SetLabelPropertyConfig; 589:GetClusterVersion; 604:SetClusterVersion; 630:GetReplicationModeConfig; 644:SetReplicationModeConfig; 664:GetPDServerConfig | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/resourcemanager/server; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/schedule/config; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/jsonutil; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/reflectutil; github.com/tikv/pd/server; github.com/tikv/pd/server/config |
| [`server/api/diagnostic.go`](../../../server/api/diagnostic.go) | 66 | 无匹配导出声明；内部实现/注册仍纳入来源 | github.com/tikv/pd/server |
| [`server/api/failpoint.go`](../../../server/api/failpoint.go) | 102 | 无匹配导出声明；内部实现/注册仍纳入来源 | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`server/api/health.go`](../../../server/api/health.go) | 43 | 35:Health; 57:GetHealthStatus; 90:Ping | github.com/tikv/pd/server; github.com/tikv/pd/server/cluster |
| [`server/api/hot_status.go`](../../../server/api/hot_status.go) | 60,61 | 51:GetHotWriteRegions; 64:GetHotReadRegions; 111:GetHotStores; 128:GetHotBuckets; 154:GetHistoryHotRegions | github.com/tikv/pd/pkg/statistics/utils; github.com/tikv/pd/server |
| [`server/api/label.go`](../../../server/api/label.go) | 51,53 | 51:GetLabels; 78:GetStoresByLabel; 116:NewStoresLabelFilter | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/response; github.com/tikv/pd/server |
| [`server/api/log.go`](../../../server/api/log.go) | 92 | 54:SetLogLevel | github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/server |
| [`server/api/member.go`](../../../server/api/member.go) | 44,45 | 58:GetMembers; 125:DeleteMemberByName; 173:DeleteMemberByID; 211:SetMemberPropertyByName; 271:GetLeader; 283:ResignLeader; 302:TransferLeader | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/etcdutil; github.com/tikv/pd/pkg/utils/keypath; github.com/tikv/pd/server |
| [`server/api/metric.go`](../../../server/api/metric.go) | 102 | 200:Header; 211:WriteHeader; 219:Unwrap | github.com/tikv/pd/pkg/ratelimit; github.com/tikv/pd/server |
| [`server/api/middleware.go`](../../../server/api/middleware.go) | 38,41,79,80,81 | 62:ServeHTTP; 214:ServeHTTP; 264:ServeHTTP | github.com/tikv/pd/pkg/audit; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/requestutil; github.com/tikv/pd/server; github.com/tikv/pd/server/cluster |
| [`server/api/min_resolved_ts.go`](../../../server/api/min_resolved_ts.go) | 77 | 43:MinResolvedTS; 59:GetStoreMinResolvedTS; 96:GetMinResolvedTS | github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/server |
| [`server/api/operator.go`](../../../server/api/operator.go) | 62,63 | 52:GetOperatorsByRegion; 80:GetOperators; 117:DeleteOperators; 138:CreateOperator; 166:DeleteOperatorByRegion; 193:GetOperatorRecords | github.com/tikv/pd/pkg/schedule/operator; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`server/api/plugin.go`](../../../server/api/plugin.go) | 102 | 54:LoadPlugin; 70:UnloadPlugin | github.com/tikv/pd/pkg/schedule; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`server/api/plugin_disable.go`](../../../server/api/plugin_disable.go) | 102 | 无匹配导出声明；内部实现/注册仍纳入来源 | github.com/tikv/pd/server |
| [`server/api/pprof.go`](../../../server/api/pprof.go) | 102 | 58:PProfZip; 157:PProfProfile; 166:PProfTrace; 175:PProfSymbol; 184:PProfHeap; 193:PProfMutex; 202:PProfAllocs; 211:PProfBlock; 220:PProfGoroutine; 229:PProfThreadcreate | github.com/tikv/pd/pkg/versioninfo; github.com/tikv/pd/server |
| [`server/api/region.go`](../../../server/api/region.go) | 55,56,57,58,59,67 | 64:GetRegionByID; 106:GetRegion; 146:CheckRegionsReplicated; 179:GetRegions; 206:ScanRegions; 238:GetRegionCount; 253:GetStoreRegions; 291:GetKeyspaceRegions; 339:GetMissPeerRegions; 370:GetExtraPeerRegions; 382:GetPendingPeerRegions; 394:GetDownPeerRegions; 406:GetLearnerPeerRegions; 418:GetOfflinePeerRegions; 430:GetOverSizedRegions; 442:GetUndersizedRegions; 454:GetEmptyRegions; 459:HistItem; 467:Len; 471:Swap; 475:Less; 488:GetSizeHistogram; 514:GetKeysHistogram; 573:GetRangeHoles; 588:GetRegionSiblings; 626:GetTopWriteFlowRegions; 639:GetTopWriteQueryRegions; 654:GetTopReadFlowRegions; 667:GetTopReadQueryRegions; 682:GetTopConfVerRegions; 697:GetTopVersionRegions; 712:GetTopSizeRegions; 727:GetTopKeysRegions; 742:GetTopCPURegions; 759:AccelerateRegionsScheduleInRange; 796:AccelerateRegionsScheduleInRanges; 860:ScatterRegions; 901:SplitRegions; 942:RegionHeap; 947:Len; 948:Less; 949:Swap; 952:Push; 958:Pop; 967:Min; 975:TopNRegions | github.com/tikv/pd/pkg/core; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/keyspace; github.com/tikv/pd/pkg/response; github.com/tikv/pd/pkg/statistics; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/server |
| [`server/api/region_label.go`](../../../server/api/region_label.go) | 70 | 50:GetAllRegionLabelRules; 67:PatchRegionLabelRules; 94:GetRegionLabelRulesByIDs; 117:GetRegionLabelRuleByID; 142:DeleteRegionLabelRule; 172:SetRegionLabelRule; 200:GetRegionLabelByKey; 227:GetRegionLabels | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/schedule/labeler; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`server/api/replication_mode.go`](../../../server/api/replication_mode.go) | 102 | 44:GetReplicationModeStatus | github.com/tikv/pd/server |
| [`server/api/router.go`](../../../server/api/router.go) | 2,81,101 | 无匹配导出声明；内部实现/注册仍纳入来源 | github.com/tikv/pd/pkg/audit; github.com/tikv/pd/pkg/ratelimit; github.com/tikv/pd/pkg/tso; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`server/api/rule.go`](../../../server/api/rule.go) | 68,69 | 92:GetAllRules; 109:SetAllRules; 143:GetRuleByGroup; 162:GetRulesByRegion; 186:CheckRegionPlacementRule; 216:GetRulesByKey; 239:GetRuleByGroupAndID; 262:SetRule; 314:DeleteRuleByGroup; 341:BatchRules; 370:GetGroupConfig; 393:SetGroupConfig; 420:DeleteGroupConfig; 444:GetAllGroupConfigs; 459:GetPlacementRules; 476:SetPlacementRules; 505:GetPlacementRuleByGroup; 524:DeletePlacementRuleByGroup; 551:SetPlacementRuleByGroup | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/schedule/placement; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`server/api/scheduler.go`](../../../server/api/scheduler.go) | 64,65 | 67:GetSchedulers; 90:CreateScheduler; 364:DeleteScheduler; 424:PauseOrResumeScheduler | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/schedule/schedulers; github.com/tikv/pd/pkg/schedule/types; github.com/tikv/pd/pkg/slice; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/keyutil; github.com/tikv/pd/server |
| [`server/api/server.go`](../../../server/api/server.go) | 102 | 39:NewHandler | github.com/tikv/pd/pkg/mcs/scheduling/server/apis/v1; github.com/tikv/pd/pkg/mcs/tso/server/apis/v1; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/apiutil/serverapi; github.com/tikv/pd/server |
| [`server/api/service_gc_safepoint.go`](../../../server/api/service_gc_safepoint.go) | 102 | 46:ListServiceGCSafepoint; 60:GetGCSafePoint; 122:DeleteGCSafePoint | github.com/tikv/pd/pkg/keyspace/constant; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/pkg/utils/keypath; github.com/tikv/pd/server |
| [`server/api/service_middleware.go`](../../../server/api/service_middleware.go) | 3,79,80 | 56:GetServiceMiddlewareConfig; 71:SetServiceMiddlewareConfig; 151:SetRateLimitConfig; 247:SetGRPCRateLimitConfig | github.com/tikv/pd/pkg/ratelimit; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/jsonutil; github.com/tikv/pd/pkg/utils/reflectutil; github.com/tikv/pd/server; github.com/tikv/pd/server/config |
| [`server/api/stats.go`](../../../server/api/stats.go) | 57,58 | 50:GetRegionStatus | github.com/tikv/pd/pkg/statistics; github.com/tikv/pd/server |
| [`server/api/status.go`](../../../server/api/status.go) | 46 | 44:GetPDStatus | github.com/tikv/pd/pkg/versioninfo; github.com/tikv/pd/server |
| [`server/api/store.go`](../../../server/api/store.go) | 51,52,53,54 | 64:GetStore; 96:DeleteStore; 128:SetStoreState; 182:SetStoreLabel; 235:DeleteStoreLabel; 277:SetStoreWeight; 333:SetStoreLimit; 407:RemoveTombStone; 430:SetAllStoresLimit; 514:GetAllStoresLimit; 547:Progress; 564:GetStoresProgress; 617:GetAllStores; 661:GetStoresByState; 711:NewStoreStateFilter; 739:Filter | github.com/tikv/pd/pkg/core; github.com/tikv/pd/pkg/core/storelimit; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/response; github.com/tikv/pd/pkg/schedule/config; github.com/tikv/pd/pkg/slice; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`server/api/tso.go`](../../../server/api/tso.go) | 75,76 | 46:TransferLocalTSOAllocator | github.com/tikv/pd/server |
| [`server/api/unsafe_operation.go`](../../../server/api/unsafe_operation.go) | 102 | 61:RemoveFailedStores; 121:AbortFailedStoresRemoval; 204:GetFailedStoresRemovalStatus | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/unsaferecovery; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/server |
| [`server/api/version.go`](../../../server/api/version.go) | 46 | 49:GetVersion | github.com/tikv/pd/pkg/versioninfo |
| [`server/apiv2/handlers/affinity.go`](../../../server/apiv2/handlers/affinity.go) | 2,71,72,78 | 32:RegisterAffinity; 47:AffinityKeyRange; 61:CreateAffinityGroupInput; 66:CreateAffinityGroupsRequest; 71:AffinityGroupsResponse; 76:BatchDeleteAffinityGroupsRequest; 82:GroupRangesModification; 88:BatchModifyAffinityGroupsRequest; 94:UpdateAffinityGroupPeersRequest; 114:PostAffinityGroups; 277:BatchModifyAffinityGroups; 352:UpdateAffinityGroupPeers; 398:DeleteAffinityGroup; 453:GetAllAffinityGroups; 491:GetAffinityGroup | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/schedule/affinity; github.com/tikv/pd/pkg/utils/keyutil; github.com/tikv/pd/server; github.com/tikv/pd/server/apiv2/middlewares |
| [`server/apiv2/handlers/keyspace.go`](../../../server/apiv2/handlers/keyspace.go) | 2,78 | 39:RegisterKeyspace; 53:CreateKeyspaceParams; 68:CreateKeyspace; 95:CreateKeyspaceByIDParams; 111:CreateKeyspaceByID; 147:LoadKeyspace; 186:LoadKeyspaceByID; 245:LoadAllKeyspacesResponse; 263:LoadAllKeyspaces; 310:UpdateConfigParams; 333:UpdateKeyspaceConfig; 406:UpdateStateParam; 422:UpdateKeyspaceState; 450:KeyspaceMeta; 456:MarshalJSON; 475:UnmarshalJSON | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/keyspace; github.com/tikv/pd/server; github.com/tikv/pd/server/apiv2/middlewares |
| [`server/apiv2/handlers/maintenance.go`](../../../server/apiv2/handlers/maintenance.go) | 2,78 | 40:RegisterMaintenance; 60:StartMaintenanceTask; 111:GetMaintenanceTasks; 150:GetMaintenanceTaskByType; 181:DeleteMaintenanceTask | github.com/tikv/pd/pkg/maintenance; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/pkg/utils/keypath; github.com/tikv/pd/server; github.com/tikv/pd/server/apiv2/middlewares |
| [`server/apiv2/handlers/meta_service_group.go`](../../../server/apiv2/handlers/meta_service_group.go) | 2,78 | 38:RegisterMetaServiceGroup; 48:MetaServiceGroupStatus; 71:PatchMetaServiceGroups; 172:GetMetaServiceGroups; 201:PatchMetaServiceGroupStatus | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/keyspace; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/server; github.com/tikv/pd/server/apiv2/middlewares; github.com/tikv/pd/server/config |
| [`server/apiv2/handlers/microservice.go`](../../../server/apiv2/handlers/microservice.go) | 2,78 | 38:RegisterMicroservice; 52:GetMembers; 79:GetPrimary; 107:EvictTSOPrimary | github.com/tikv/pd/pkg/mcs/discovery; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/grpcutil; github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/server; github.com/tikv/pd/server/apiv2/middlewares |
| [`server/apiv2/handlers/ready.go`](../../../server/apiv2/handlers/ready.go) | 2,43,78,94 | 38:RegisterReadyCheck; 68:NewCheckRegistry; 244:ReadyStatus; 258:Ready | github.com/tikv/pd/pkg/storage; github.com/tikv/pd/server; github.com/tikv/pd/server/apiv2/middlewares |
| [`server/apiv2/handlers/safe_point.go`](../../../server/apiv2/handlers/safe_point.go) | 2,78 | 28:RegisterSafePoint; 35:GCSafePoint; 50:LoadGCSafePoint | github.com/tikv/pd/server; github.com/tikv/pd/server/apiv2/middlewares |
| [`server/apiv2/handlers/tso_keyspace_group.go`](../../../server/apiv2/handlers/tso_keyspace_group.go) | 2,78 | 44:RegisterTSOKeyspaceGroup; 62:CreateKeyspaceGroupParams; 67:CreateKeyspaceGroups; 115:GetKeyspaceGroups; 160:GetKeyspaceGroupPrimaryResponse; 176:GetKeyspaceGroupByID; 251:DeleteKeyspaceGroupByID; 277:SplitKeyspaceGroupByIDParams; 292:SplitKeyspaceGroupByID; 355:FinishSplitKeyspaceByID; 377:MergeKeyspaceGroupsParams; 383:MergeKeyspaceGroups; 430:FinishMergeKeyspaceByID; 452:AllocNodesForKeyspaceGroupParams; 457:AllocNodesForKeyspaceGroup; 507:SetNodesForKeyspaceGroupParams; 512:SetNodesForKeyspaceGroup; 553:SetPriorityForKeyspaceGroupParams; 558:SetPriorityForKeyspaceGroup; 632:RemoveKeyspacesFromGroupParams; 638:RemoveKeyspacesFromGroup | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/keyspace/constant; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/slice; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/pkg/utils/syncutil; github.com/tikv/pd/server; github.com/tikv/pd/server/apiv2/middlewares |
| [`server/apiv2/middlewares/bootstrap_checker.go`](../../../server/apiv2/middlewares/bootstrap_checker.go) | 102 | 30:BootstrapChecker | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/server |
| [`server/apiv2/middlewares/microservice_redirector.go`](../../../server/apiv2/middlewares/microservice_redirector.go) | 102 | 32:MicroserviceRedirector; 77:AffinityMicroserviceRedirector | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/scheduling/server/apis/v1; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/apiutil/serverapi; github.com/tikv/pd/server |
| [`server/apiv2/middlewares/redirector.go`](../../../server/apiv2/middlewares/redirector.go) | 41 | 32:Redirector | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server |
| [`server/apiv2/router.go`](../../../server/apiv2/router.go) | 2,78,101 | 49:NewV2Handler | github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/server; github.com/tikv/pd/server/apiv2/handlers; github.com/tikv/pd/server/apiv2/middlewares |
| [`server/cluster/bucket_heartbeat.rs`](../../../server/cluster/bucket_heartbeat.rs) | 1,10,27,34,77 | 9:pub struct BucketRegionNotFound(pub u64);; 17:pub(super) fn retry_bucket_update(mut update: impl FnMut() -> bool) -> bool {; 32:pub fn handle_bucket_heartbeat( | use astersql_pd_core::{BasicCluster, RegionBucketStats, RegionBuckets};; use astersql_pd_statistics_buckets::{BucketReport, BucketWorkerClient};; use std::sync::Arc; |
| [`server/cluster/cluster.go`](../../../server/cluster/cluster.go) | 1,6,12,13,14,15,16,20,27,34,77 | 134:Server; 159:RaftCluster; 224:SetOnStoreBuried; 232:Status; 239:NewRaftCluster; 271:GetStoreConfig; 276:GetCheckerConfig; 281:GetSchedulerConfig; 286:GetSharedConfig; 291:LoadClusterStatus; 336:InitCluster; 357:Start; 873:LoadClusterInfo; 986:Stop; 1044:Wait; 1049:IsRunning; 1056:Context; 1066:GetHeartbeatStreams; 1071:AllocID; 1076:GetPrepareRegionCount; 1081:GetRegionSyncer; 1086:GetReplicationMode; 1091:GetRuleManager; 1096:GetKeyRangeManager; 1101:GetRegionLabeler; 1106:GetAffinityManager; 1111:GetStorage; 1117:GetOpts; 1122:GetScheduleConfig; 1127:SetScheduleConfig; 1132:GetReplicationConfig; 1137:GetPDServerConfig; 1142:SetPDServerConfig; 1150:IsSchedulingHalted; 1155:GetUnsafeRecoveryController; 1160:HandleStoreHeartbeat; 1504:GetBasicCluster; 1510:UpdateStoreLabels; 1527:DeleteStoreLabel; 1557:PutMetaStore; 1705:RemoveStore; 1814:BuryStore; 1825:BuryStoreLocked; 1880:NeedAwakenAllRegionsInStore; 1909:UpStore; 1958:ReadyToServeLocked; 1985:SetStoreWeight; 2244:RemoveTombStoneRecords; 2361:OnStoreVersionChange; 2401:GetMetaCluster; 2408:PutMetaCluster; 2439:GetHotRegionStatusByRange; 2498:GetRegionStatsByRange; 2504:GetRegionStatsCount; 2523:GetStoreLimitByType; 2528:GetAllStoresLimit; 2539:AddStoreLimit; 2596:RemoveStoreLimit; 2624:SetMinResolvedTS; 2643:CheckAndUpdateMinResolvedTS; 2709:GetMinResolvedTS; 2717:GetStoreMinResolvedTS; 2730:GetMinResolvedTSByStoreIDs; 2744:GetExternalTS; 2752:SetExternalTS; 2768:SetStoreLimit; 2814:SetAllStoresLimit; 2858:GetClusterVersion; 2863:GetEtcdClient; 2868:GetProgressByID; 2880:GetProgressByAction; 2894:CheckHealth; 2922:GetMembers; 2946:IsClientURL; 2962:IsServiceIndependent; 2968:SetServiceIndependent; 2973:UnsetServiceIndependent | github.com/tikv/pd/pkg/cluster; github.com/tikv/pd/pkg/core; github.com/tikv/pd/pkg/core/storelimit; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/gc; github.com/tikv/pd/pkg/gctuner; github.com/tikv/pd/pkg/id; github.com/tikv/pd/pkg/keyspace; github.com/tikv/pd/pkg/mcs/discovery; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/member; github.com/tikv/pd/pkg/memory; github.com/tikv/pd/pkg/metering; github.com/tikv/pd/pkg/progress; github.com/tikv/pd/pkg/ratelimit; github.com/tikv/pd/pkg/replication; github.com/tikv/pd/pkg/schedule; github.com/tikv/pd/pkg/schedule/affinity; github.com/tikv/pd/pkg/schedule/config; github.com/tikv/pd/pkg/schedule/filter; github.com/tikv/pd/pkg/schedule/hbstream; github.com/tikv/pd/pkg/schedule/keyrange; github.com/tikv/pd/pkg/schedule/labeler; github.com/tikv/pd/pkg/schedule/placement; github.com/tikv/pd/pkg/schedule/scatter; github.com/tikv/pd/pkg/schedule/schedulers; github.com/tikv/pd/pkg/statistics; github.com/tikv/pd/pkg/statistics/utils; github.com/tikv/pd/pkg/storage; github.com/tikv/pd/pkg/syncer; github.com/tikv/pd/pkg/tso; github.com/tikv/pd/pkg/unsaferecovery; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/etcdutil; github.com/tikv/pd/pkg/utils/keypath; github.com/tikv/pd/pkg/utils/keyutil; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/netutil; github.com/tikv/pd/pkg/utils/syncutil; github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/pkg/versioninfo; github.com/tikv/pd/server/config |
| [`server/cluster/cluster_worker.go`](../../../server/cluster/cluster_worker.go) | 1,27,34,77 | 40:HandleRegionHeartbeat; 78:HandleAskSplit; 123:HandleAskBatchSplit; 232:HandleReportSplit; 256:HandleBatchReportSplit; 278:HandleRegionBuckets | github.com/tikv/pd/pkg/core; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/ratelimit; github.com/tikv/pd/pkg/schedule/hbstream; github.com/tikv/pd/pkg/schedule/operator; github.com/tikv/pd/pkg/statistics/buckets; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/pkg/versioninfo |
| [`server/cluster/heartbeat_effects.rs`](../../../server/cluster/heartbeat_effects.rs) | 1,9,27,34,77 | 16:pub struct HeartbeatEffectsState<C> {; 26:pub struct HeartbeatRunners<'a> {; 33:pub enum HeartbeatEffect {; 43:pub enum HeartbeatEffectFailure {; 52:pub struct HeartbeatEffectsReceipt {; 59:pub async fn wait(&mut self) -> Vec<HeartbeatEffectFailure> {; 76:pub fn submit_region_heartbeat<C: CheckerConfigProvider + Send + 'static>( | use crate::{RegionHeartbeatResult, prepare_region_heartbeat};; use astersql_pd_core::{BasicCluster, RegionInfo, StaleRegionError};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_ratelimit::{ConcurrentRunner, RunnerError};; use astersql_pd_schedule_config::CheckerConfigProvider;; use astersql_pd_schedule_placement::RuleManager;; use astersql_pd_statistics::{LabelStatistics, RegionStatistics};; use astersql_pd_storage_endpoint::{RegionMetaError, StorageEndpoint};; use astersql_pd_syncer::{Downstreams, HistoryError};; use std::sync::{Arc, Mutex};; use tokio::sync::oneshot; |
| [`server/cluster/lib.rs`](../../../server/cluster/lib.rs) | 1,27,34,77 | 6:pub use store_heartbeat::{StoreNotFound, handle_store_heartbeat};; 16:pub use region_heartbeat::{; 24:pub use region_overlap::handle_region_heartbeat_with_overlap_cleanup;; 29:pub use bucket_heartbeat::{BucketRegionNotFound, handle_bucket_heartbeat};; 34:pub use heartbeat_effects::{; 46:pub use state_pipeline::{StatePipeline, StateSnapshot}; | pub use store_heartbeat::{StoreNotFound, handle_store_heartbeat};; pub use region_heartbeat::{; pub use region_overlap::handle_region_heartbeat_with_overlap_cleanup;; pub use bucket_heartbeat::{BucketRegionNotFound, handle_bucket_heartbeat};; pub use heartbeat_effects::{; pub use state_pipeline::{StatePipeline, StateSnapshot}; |
| [`server/cluster/metering.go`](../../../server/cluster/metering.go) | 1,27,34,77 | 72:Category; 75:Collect; 85:Aggregate | github.com/tikv/pd/pkg/metering |
| [`server/cluster/metrics.go`](../../../server/cluster/metrics.go) | 1,27,34,77 | 无匹配导出声明；内部实现/注册仍纳入来源 | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`server/cluster/region_heartbeat.rs`](../../../server/cluster/region_heartbeat.rs) | 1,27,34,77 | 12:pub struct PreparedRegionHeartbeat {; 18:pub struct RegionHeartbeatResult {; 27:pub fn prepare_region_heartbeat(; 45:pub fn commit(; 54:pub(crate) fn commit_root(; 73:pub fn handle_region_heartbeat( | use astersql_pd_core::{BasicCluster, RegionGuide, RegionInfo, StaleRegionError, region_guide};; use std::sync::Arc; |
| [`server/cluster/region_overlap.rs`](../../../server/cluster/region_overlap.rs) | 1,27,34,77 | 12:pub fn handle_region_heartbeat_with_overlap_cleanup( | use crate::{RegionHeartbeatResult, handle_region_heartbeat};; use astersql_pd_core::{BasicCluster, RegionInfo, StaleRegionError};; use astersql_pd_statistics::{LabelStatistics, RegionStatistics}; |
| [`server/cluster/removed_store.rs`](../../../server/cluster/removed_store.rs) | 1,27,34,77 | 9:pub(super) fn handle_checked( | use super::StoreNotFound;; use astersql_pd_core::{BasicCluster, NodeState, StoreStats};; use astersql_pd_statistics::{HotStat, HotTask};; use astersql_pd_statistics_utils::RWType;; use std::collections::BTreeSet; |
| [`server/cluster/scheduling_controller.go`](../../../server/cluster/scheduling_controller.go) | 1,27,34,77 | 266:GetHotStat; 271:GetRegionStats; 276:GetLabelStats; 281:GetRegionStatsByType; 289:UpdateRegionsLabelLevelStats; 308:GetStoresStats; 313:GetStoresLoads; 318:GetStoreReadCPURecentMax; 323:IsRegionHot; 328:GetHotPeerStat; 335:GetHotPeerStats; 346:BucketsStats; 351:GetCoordinator; 358:GetPausedSchedulerDelayAt; 365:GetPausedSchedulerDelayUntil; 372:GetOperatorController; 379:GetRegionScatterer; 386:GetRegionSplitter; 393:GetMergeChecker; 400:GetRuleChecker; 407:GetSchedulers; 414:GetSchedulerHandlers; 421:AddSchedulerHandler; 428:RemoveSchedulerHandler; 435:AddScheduler; 442:RemoveScheduler; 449:PauseOrResumeScheduler; 456:PauseOrResumeChecker; 463:AddPendingProcessedRegions; 470:GetPendingProcessedRegions; 477:RemovePendingProcessedRegion; 486:PopOneSuspectKeyRange; 493:ClearSuspectKeyRanges; 502:AddSuspectKeyRange; 529:IsPrepared; 539:SetPrepared; 546:IsSchedulingControllerRunning | github.com/tikv/pd/pkg/core; github.com/tikv/pd/pkg/schedule; github.com/tikv/pd/pkg/schedule/checker; github.com/tikv/pd/pkg/schedule/config; github.com/tikv/pd/pkg/schedule/core; github.com/tikv/pd/pkg/schedule/filter; github.com/tikv/pd/pkg/schedule/hbstream; github.com/tikv/pd/pkg/schedule/operator; github.com/tikv/pd/pkg/schedule/placement; github.com/tikv/pd/pkg/schedule/scatter; github.com/tikv/pd/pkg/schedule/schedulers; github.com/tikv/pd/pkg/schedule/splitter; github.com/tikv/pd/pkg/schedule/types; github.com/tikv/pd/pkg/statistics; github.com/tikv/pd/pkg/statistics/buckets; github.com/tikv/pd/pkg/statistics/utils; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/syncutil |
| [`server/cluster/state_pipeline.rs`](../../../server/cluster/state_pipeline.rs) | 1,5,6,12,13,14,15,16,27,34,77 | 19:pub struct StatePipeline<'a, C> {; 31:pub struct StateSnapshot {; 40:pub fn store(; 48:pub fn bucket(&self, report: BucketReport) -> Result<bool, BucketRegionNotFound> {; 54:pub fn region(; 65:pub fn region_with_witness(; 92:pub async fn snapshot(&self) -> Result<StateSnapshot, StatisticsSnapshotError> { | use crate::{; use astersql_pd_core::{NodeState, RegionInfo, StaleRegionError, StoreRegionStats, StoreStats};; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_config::CheckerConfigProvider;; use astersql_pd_statistics::{HotStat, StatisticsSnapshot, StatisticsSnapshotError};; use astersql_pd_statistics_buckets::BucketReport;; use astersql_pd_storage_endpoint::StorageEndpoint;; use astersql_pd_syncer::Downstreams;; use std::sync::{Arc, Mutex}; |
| [`server/cluster/store_heartbeat.rs`](../../../server/cluster/store_heartbeat.rs) | 1,27,34,77 | 8:pub struct StoreNotFound(pub u64);; 25:pub fn handle_store_heartbeat(; 35:pub(super) fn peer_loads(peer: &PeerStat, interval: u64) -> [f64; 8] { | use astersql_pd_core::{BasicCluster, NodeState, PeerStat, StoreStats};; use astersql_pd_statistics::{HotStat, region_read_cpu_usage}; |
| [`server/config/config.go`](../../../server/config/config.go) | 3,28,47,83,84,85,90 | 50:Config; 172:NewConfig; 280:Parse; 334:Validate; 358:Adjust; 494:Clone; 499:String; 509:PDServerConfig; 615:Clone; 623:Validate; 666:StoreLabel; 673:LabelPropertyConfig; 676:Clone; 687:GetLease; 693:GetMaxConcurrentTSOProxyStreamings; 698:GetTSOProxyRecvFromClientTimeout; 703:GetTSOUpdatePhysicalInterval; 708:GetTSOSaveInterval; 713:GetTLSConfig; 718:GenEmbedEtcdConfig; 780:DashboardConfig; 793:ToTiDBTLSConfig; 817:ReplicationModeConfig; 823:Clone; 837:NormalizeReplicationMode; 846:DRAutoSyncReplicationConfig; 864:MicroserviceConfig; 886:Clone; 892:IsSchedulingFallbackEnabled; 897:IsTSODynamicSwitchingEnabled; 902:IsResourceManagerFallbackEnabled; 907:KeyspaceConfig; 922:Validate; 951:IsValidMetaServiceGroupID; 956:AdjustMetaServiceGroups; 987:Clone; 1000:GetPreAlloc; 1007:ToWaitRegionSplit; 1012:GetWaitRegionSplitTimeout; 1017:GetCheckRegionSplitInterval; 1022:GetMetaServiceGroups; 1031:SetMetaServiceGroups | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/resourcemanager/server; github.com/tikv/pd/pkg/schedule/config; github.com/tikv/pd/pkg/utils/configutil; github.com/tikv/pd/pkg/utils/grpcutil; github.com/tikv/pd/pkg/utils/metricutil; github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/pkg/versioninfo |
| [`server/config/config_decode.rs`](../../../server/config/config_decode.rs) | 3,28,47,83,84,85,90 | 12:pub struct ConfigScalars {; 76:pub struct DecodedScalars {; 84:pub fn decode_json_scalars(input: &str) -> Result<DecodedScalars, serde_json::Error> {; 94:pub fn decode_toml_scalars(input: &str) -> Result<DecodedScalars, toml::de::Error> { | use astersql_pd_configutil::ConfigMetaData;; use astersql_pd_typeutil::{ByteSize, Duration};; use serde::{Deserialize, Serialize};; use std::collections::BTreeMap; |
| [`server/config/config_defaults.rs`](../../../server/config/config_defaults.rs) | 3,28,47,83,84,85,90 | 10:pub struct ConfigDefaultsContext<'a> {; 19:pub fn adjust_base_defaults(; 74:pub(crate) fn clean_path(cwd: &Path, value: &str) -> String { | use crate::ConfigScalars;; use astersql_pd_configutil::{ConfigMetaData, adjust_duration, adjust_int64, adjust_string};; use astersql_pd_typeutil::{ByteSize, Duration};; use std::path::{Component, Path, PathBuf}; |
| [`server/config/config_tso.rs`](../../../server/config/config_tso.rs) | 3,28,47,83,84,85,90 | 12:pub fn adjust_tso_defaults(&mut self) { | use crate::ConfigScalars;; use astersql_pd_configutil::{adjust_duration, adjust_int64};; use astersql_pd_typeutil::Duration; |
| [`server/config/config_urls.rs`](../../../server/config/config_urls.rs) | 3,28,47,83,84,85,90 | 9:pub enum ConfigUrlError {; 50:pub fn validate_url_with_scheme(raw: &str) -> Result<(), ConfigUrlError> {; 61:pub fn validate_url_list(raw: &str) -> Result<(), ConfigUrlError> {; 75:pub fn validate_join(&self) -> Result<(), ConfigUrlError> { | use crate::ConfigScalars;; use astersql_pd_errs::{TypedError, catalog::ERR_URL_PARSE};; use std::{error::Error, fmt, net::Ipv6Addr}; |
| [`server/config/dashboard_config.rs`](../../../server/config/dashboard_config.rs) | 3,28,47,83,84,85,90 | 9:pub struct DashboardConfig {; 32:pub fn adjust(&mut self, metadata: &ConfigMetaData) {; 38:pub fn adjust_with_telemetry_default(&mut self, metadata: &ConfigMetaData, default: bool) {; 46:pub fn decode_toml_dashboard( | use astersql_pd_configutil::ConfigMetaData;; use serde::{Deserialize, Deserializer, Serialize}; |
| [`server/config/deprecated_config.rs`](../../../server/config/deprecated_config.rs) | 3,28,47,83,84,85,90 | 11:pub fn adjust_deprecated_from_file(; 48:pub fn migrate_configuration_from_file(metadata: &ConfigMetaData) -> Result<(), &'static str> { | use astersql_pd_configutil::ConfigMetaData;; use crate::ConfigScalars; |
| [`server/config/keyspace_config.rs`](../../../server/config/keyspace_config.rs) | 3,28,47,83,84,85,90 | 12:pub struct KeyspaceConfig {; 28:pub fn adjust(&mut self, metadata: &ConfigMetaData) -> Result<(), String> {; 45:pub fn validate(&self) -> Result<(), String> {; 60:pub fn get_pre_alloc(&self) -> Vec<String> {; 65:pub fn get_meta_service_groups(&self) -> BTreeMap<String, String> {; 70:pub fn set_meta_service_groups(&mut self, groups: Option<BTreeMap<String, String>>) {; 76:pub fn is_valid_meta_service_group_id(id: &str) -> bool {; 81:pub fn adjust_meta_service_groups(groups: &mut BTreeMap<String, String>) -> Result<(), String> {; 109:pub fn decode_toml_keyspace( | use std::collections::BTreeMap;; use astersql_pd_configutil::ConfigMetaData;; use astersql_pd_typeutil::Duration;; use serde::{Deserialize, Serialize}; |
| [`server/config/lib.rs`](../../../server/config/lib.rs) | 3,28,47,83,84,85,90 | 4:pub use config_decode::*;; 10:pub use config_urls::*;; 16:pub use config_defaults::*;; 27:pub use pd_server_defaults::*;; 38:pub use pd_server_validate::*;; 44:pub use keyspace_config::*;; 50:pub use replication_config::*;; 56:pub use microservice_config::*;; 62:pub use dashboard_config::*;; 68:pub use middleware_config::*;; 74:pub use deprecated_config::*;; 80:pub use nested_config::*;; 85:pub use persist_schedule::*; | pub use config_decode::*;; pub use config_urls::*;; pub use config_defaults::*;; pub use pd_server_defaults::*;; pub use pd_server_validate::*;; pub use keyspace_config::*;; pub use replication_config::*;; pub use microservice_config::*;; pub use dashboard_config::*;; pub use middleware_config::*;; pub use deprecated_config::*;; pub use nested_config::*;; pub use persist_schedule::*; |
| [`server/config/metrics.go`](../../../server/config/metrics.go) | 3,28,47,83,84,85,90 | 无匹配导出声明；内部实现/注册仍纳入来源 | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`server/config/microservice_config.rs`](../../../server/config/microservice_config.rs) | 3,28,47,83,84,85,90 | 9:pub struct MicroserviceConfig {; 20:pub fn adjust(&mut self, metadata: &ConfigMetaData) {; 33:pub fn is_scheduling_fallback_enabled(&self) -> bool {; 38:pub fn is_tso_dynamic_switching_enabled(&self) -> bool {; 43:pub fn is_resource_manager_fallback_enabled(&self) -> bool {; 49:pub fn decode_toml_microservice(; 74:pub fn serialize<S: Serializer>(value: &bool, serializer: S) -> Result<S::Ok, S::Error> {; 78:pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> { | use astersql_pd_configutil::ConfigMetaData;; use serde::{Deserialize, Serialize};; use serde::{Deserialize, Deserializer, Serializer}; |
| [`server/config/middleware_config.rs`](../../../server/config/middleware_config.rs) | 3,28,47,83,84,85,90 | 11:pub struct MiddlewareDimensionConfig {; 26:pub struct MiddlewareLimiterMap(Arc<RwLock<BTreeMap<String, MiddlewareDimensionConfig>>>);; 29:pub fn insert(&self, label: String, value: MiddlewareDimensionConfig) {; 36:pub fn get(&self, label: &str) -> Option<MiddlewareDimensionConfig> {; 68:pub struct AuditConfig {; 76:pub struct RateLimitConfig {; 99:pub struct GRPCRateLimitConfig {; 122:pub struct ServiceMiddlewareConfig {; 132:pub fn new() -> Self {; 165:pub fn serialize<S: Serializer>(value: &bool, serializer: S) -> Result<S::Ok, S::Error> {; 169:pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> { | use serde::{Deserialize, Deserializer, Serialize, Serializer};; use std::{; use serde::{Deserialize, Deserializer, Serializer}; |
| [`server/config/nested_config.rs`](../../../server/config/nested_config.rs) | 3,28,47,83,84,85,90 | 15:pub struct Config {; 44:pub struct StoreLabel {; 50:pub fn adjust(; 143:pub fn validate(&self, cwd: &std::path::Path) -> Result<(), String> {; 164:pub struct ClusterVersion {; 226:pub struct DecodedConfig {; 233:pub fn decode_json_config(input: &str) -> Result<DecodedConfig, serde_json::Error> {; 301:pub fn decode_toml_config(input: &str) -> Result<DecodedConfig, String> { | use crate::*;; use astersql_pd_configutil::{ConfigMetaData, SecurityConfig};; use astersql_pd_logutil::LogConfig;; use astersql_pd_metering_config::MeteringConfig;; use astersql_pd_metricutil::MetricConfig;; use astersql_pd_rm_server::ControllerConfig;; use astersql_pd_schedule_config::{ReplicationConfig, ScheduleConfig, ScheduleDefaults};; use serde::{Deserialize, Serialize};; use std::collections::BTreeMap;; use astersql_pd_configutil::{adjust_duration, adjust_string};; use astersql_pd_typeutil::{ByteSize, Duration}; |
| [`server/config/pd_server_defaults.rs`](../../../server/config/pd_server_defaults.rs) | 3,28,47,83,84,85,90 | 12:pub struct PDServerConfig {; 52:pub fn adjust_defaults(&mut self, metadata: &ConfigMetaData) {; 104:pub fn decode_toml_pd_server(; 168:pub fn serialize<S: Serializer>(value: &bool, serializer: S) -> Result<S::Ok, S::Error> {; 171:pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> { | use astersql_pd_configutil::{ConfigMetaData, adjust_duration, adjust_float64, adjust_int64};; use astersql_pd_typeutil::{Duration, StringSlice};; use serde::{Deserialize, Serialize};; use serde::{Deserialize, Deserializer, Serializer}; |
| [`server/config/pd_server_gc.rs`](../../../server/config/pd_server_gc.rs) | 3,28,47,83,84,85,90 | 8:pub fn validate_gc_concurrency(&self) -> Result<(), &'static str> { | use crate::PDServerConfig; |
| [`server/config/pd_server_validate.rs`](../../../server/config/pd_server_validate.rs) | 3,28,47,83,84,85,90 | 11:pub enum PDServerValidationError {; 48:pub fn validate(&self) -> Result<(), PDServerValidationError> { | use std::{error::Error, fmt};; use astersql_pd_errs::{ErrorTemplate, TypedError, catalog::ERR_CONFIG_ITEM};; use crate::{ConfigUrlError, PDServerConfig, validate_url_with_scheme};; use PDServerValidationError as E; |
| [`server/config/persist_options.go`](../../../server/config/persist_options.go) | 3,28,47,83,84,85,90 | 49:PersistOptions; 66:NewPersistOptions; 84:GetScheduleConfig; 89:SetScheduleConfig; 109:UpdateScheduleConfig; 131:SetSchedulers; 138:GetReplicationConfig; 143:SetReplicationConfig; 148:GetPDServerConfig; 153:SetPDServerConfig; 158:GetReplicationModeConfig; 163:SetReplicationModeConfig; 168:GetLabelPropertyConfig; 173:SetLabelPropertyConfig; 178:GetKeyspaceConfig; 183:SetKeyspaceConfig; 188:CASKeyspaceConfig; 193:GetMicroserviceConfig; 198:SetMicroserviceConfig; 203:GetStoreConfig; 208:SetStoreConfig; 213:GetClusterVersion; 218:SetClusterVersion; 223:CASClusterVersion; 228:GetLocationLabels; 233:SetLocationLabels; 240:GetIsolationLevel; 245:IsPlacementRulesEnabled; 250:GetAffinityScheduleLimit; 255:GetSplitScatterScheduleLimit; 260:SetAffinityScheduleLimit; 267:SetSplitScatterScheduleLimit; 274:SetPlacementRuleEnabled; 281:IsPlacementRulesCacheEnabled; 286:SetPlacementRulesCacheEnabled; 293:GetStrictlyMatchLabel; 298:GetMaxReplicas; 303:SetMaxReplicas; 326:IsSupportedTTLConfig; 336:GetMaxSnapshotCount; 341:GetMaxPendingPeerCount; 346:GetMaxMergeRegionSize; 351:GetMaxAffinityMergeRegionSize; 357:GetMaxMergeRegionKeys; 370:GetSplitMergeInterval; 375:SetSplitMergeInterval; 382:GetSwitchWitnessInterval; 387:IsDiagnosticAllowed; 392:SetEnableDiagnostic; 399:IsWitnessAllowed; 404:SetEnableWitness; 411:SetMaxStoreDownTime; 418:SetMaxMergeRegionSize; 425:SetMaxAffinityMergeRegionSize; 432:SetMaxMergeRegionKeys; 439:SetStoreLimit; 450:SetAllStoresLimit; 461:IsOneWayMergeEnabled; 466:IsCrossTableMergeEnabled; 471:GetPatrolRegionInterval; 476:GetMaxStoreDownTime; 481:GetMaxStorePreparingTime; 486:GetLeaderScheduleLimit; 491:GetRegionScheduleLimit; 496:GetWitnessScheduleLimit; 501:GetReplicaScheduleLimit; 506:GetMergeScheduleLimit; 511:GetHotRegionScheduleLimit; 516:GetStoreLimit; 532:GetStoreLimitByType; 546:PeekStoreLimitByType; 572:GetAllStoresLimit; 577:GetStoreLimitVersion; 582:GetTolerantSizeRatio; 587:GetLowSpaceRatio; 592:GetSlowStoreEvictingAffectedStoreRatioThreshold; 597:GetHighSpaceRatio; 602:GetRegionScoreFormulaVersion; 607:GetSchedulerMaxWaitingOperator; 612:GetLeaderSchedulePolicy; 617:GetKeyType; 622:GetMaxResetTSGap; 627:GetDashboardAddress; 632:IsUseRegionStorage; 637:GetServerMemoryLimit; 642:GetServerMemoryLimitGCTrigger; 647:GetEnableGOGCTuner; 652:GetGCTunerThreshold; 657:IsRemoveDownReplicaEnabled; 662:IsReplaceOfflineReplicaEnabled; 667:IsMakeUpReplicaEnabled; 672:IsRemoveExtraReplicaEnabled; 677:IsLocationReplacementEnabled; 682:IsTikvRegionSplitEnabled; 687:GetMaxMovableHotPeerSize; 692:IsDebugMetricsEnabled; 697:IsUseJointConsensus; 702:SetEnableUseJointConsensus; 710:IsTraceRegionFlow; 715:GetHotRegionCacheHitsThreshold; 720:GetPatrolRegionWorkerCount; 725:GetStoresLimit; 730:GetSchedulers; 735:IsSchedulerDisabled; 747:GetHotRegionsWriteInterval; 752:GetHotRegionsReservedDays; 757:AddSchedulerCfg; 779:RemoveSchedulerCfg; 797:SetLabelProperty; 809:DeleteLabelProperty; 834:UnmarshalJSON; 852:SwitchRaftV2; 860:Persist; 887:Reload; 931:CheckLabelProperty; 944:GetMinResolvedTSPersistenceInterval; 949:SetTTLData; 1021:GetTTLData; 1032:LoadTTLFromEtcd; 1056:SetSchedulingAllowanceStatus; 1067:SetHaltScheduling; 1075:IsSchedulingHalted; 1083:GetRegionMaxSize; 1088:GetRegionMaxKeys; 1093:GetRegionSplitSize; 1098:GetRegionSplitKeys; 1103:CheckRegionSize; 1108:CheckRegionKeys; 1113:IsEnableRegionBucket; 1118:IsRaftKV2; 1123:GetUnifiedReadPoolMaxThreadCount; 1129:SetRegionBucketEnabled; 1136:GetRegionBucketSize | github.com/tikv/pd/pkg/cache; github.com/tikv/pd/pkg/core/constant; github.com/tikv/pd/pkg/core/storelimit; github.com/tikv/pd/pkg/schedule/config; github.com/tikv/pd/pkg/schedule/types; github.com/tikv/pd/pkg/slice; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/pkg/utils/etcdutil; github.com/tikv/pd/pkg/utils/typeutil |
| [`server/config/persist_schedule.rs`](../../../server/config/persist_schedule.rs) | 3,28,47,48,49,50,83,84,85,90 | 14:pub struct PersistedConfig {; 21:pub enum ConfigPersistenceError {; 29:pub async fn persist(; 42:pub async fn reload(; 223:pub struct HotConfig {; 231:pub fn new(config: PersistedConfig, endpoint: StorageEndpoint, paths: ClusterPaths) -> Self {; 240:pub fn snapshot(&self) -> std::sync::Arc<PersistedConfig> {; 245:pub async fn update_schedule(; 272:pub async fn mutate_schedule(; 282:pub async fn persist(&self) -> Result<(), ConfigPersistenceError> { | use crate::{Config, ConfigDefaultsContext};; use astersql_pd_configutil::ConfigMetaData;; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_schedule_config::{ScheduleDefaults, StoreConfig};; use astersql_pd_storage_endpoint::{EndpointError, StorageEndpoint};; use serde::{Deserialize, Serialize};; use serde_json::Value; |
| [`server/config/replication_config.rs`](../../../server/config/replication_config.rs) | 3,28,47,83,84,85,90 | 10:pub struct ReplicationModeConfig {; 20:pub struct DRAutoSyncReplicationConfig {; 39:pub fn adjust(&mut self, metadata: &ConfigMetaData) {; 51:pub fn adjust(&mut self, metadata: &ConfigMetaData) {; 59:pub fn normalize_replication_mode(mode: &str) -> String {; 68:pub fn decode_toml_replication_mode(; 97:pub fn serialize<S: Serializer>(value: &bool, serializer: S) -> Result<S::Ok, S::Error> {; 101:pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> { | use astersql_pd_configutil::ConfigMetaData;; use astersql_pd_typeutil::Duration;; use serde::{Deserialize, Serialize};; use serde::{Deserialize, Deserializer, Serializer}; |
| [`server/config/service_middleware_config.go`](../../../server/config/service_middleware_config.go) | 3,28,47,83,84,85,90 | 26:ServiceMiddlewareConfig; 33:NewServiceMiddlewareConfig; 54:Clone; 60:AuditConfig; 66:Clone; 72:RateLimitConfig; 80:Clone; 91:GRPCRateLimitConfig; 99:Clone | github.com/tikv/pd/pkg/ratelimit |
| [`server/config/service_middleware_persist_options.go`](../../../server/config/service_middleware_persist_options.go) | 3,28,47,83,84,85,90 | 28:ServiceMiddlewarePersistOptions; 35:NewServiceMiddlewarePersistOptions; 44:GetAuditConfig; 49:SetAuditConfig; 54:IsAuditEnabled; 59:GetRateLimitConfig; 64:SetRateLimitConfig; 69:IsRateLimitEnabled; 74:GetGRPCRateLimitConfig; 79:SetGRPCRateLimitConfig; 84:IsGRPCRateLimitEnabled; 89:Persist; 103:Reload | github.com/tikv/pd/pkg/storage/endpoint |
| [`server/config/util.go`](../../../server/config/util.go) | 3,28,47,83,84,85,90 | 27:ValidateURLWithScheme | github.com/tikv/pd/pkg/errs |
| [`server/grpc_service.go`](../../../server/grpc_service.go) | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | 79:GrpcServer; 98:Send; 163:Send; 187:Recv; 254:GetClusterInfo; 280:GetMinTS; 325:GetMinTSFromTSOService; 434:GetMembers; 498:Tso; 638:Bootstrap; 678:IsBootstrapped; 703:AllocID; 747:IsSnapshotRecovering; 774:GetStore; 826:PutStore; 886:GetAllStores; 927:StoreHeartbeat; 1092:ReportBuckets; 1281:RegionHeartbeat; 1514:GetRegion; 1547:GetPrevRegion; 1577:GetRegionByID; 1606:QueryRegion; 1676:ScanRegions; 1705:BatchScanRegions; 1735:AskSplit; 1777:AskBatchSplit; 1841:ReportSplit; 1875:ReportBatchSplit; 1910:GetClusterConfig; 1938:PutClusterConfig; 1975:ScatterRegion; 2086:SyncRegions; 2105:GetOperator; 2291:SyncMaxTS; 2298:SplitRegions; 2360:SplitAndScatterRegions; 2420:GetDCLocationInfo; 2457:StoreGlobalConfig; 2501:LoadGlobalConfig; 2553:WatchGlobalConfig; 2662:ReportMinResolvedTS; 2698:SetExternalTimestamp; 2732:GetExternalTimestamp | github.com/tikv/pd/pkg/core; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/ratelimit; github.com/tikv/pd/pkg/storage/kv; github.com/tikv/pd/pkg/utils/grpcutil; github.com/tikv/pd/pkg/utils/keypath; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/syncutil; github.com/tikv/pd/pkg/utils/tsoutil; github.com/tikv/pd/pkg/versioninfo; github.com/tikv/pd/server/cluster |
| [`server/join/client_url.go`](../../../server/join/client_url.go) | 39,88 | 无匹配导出声明；内部实现/注册仍纳入来源 | github.com/tikv/pd/pkg/slice |
| [`server/join/join.go`](../../../server/join/join.go) | 88,89 | 89:PrepareJoinCluster | github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/slice; github.com/tikv/pd/pkg/utils/etcdutil; github.com/tikv/pd/server/config |
| [`server/server.go`](../../../server/server.go) | 1,7,82,94 | 136:Server; 255:HandlerBuilder; 258:CreateServer; 482:AddStartCallback; 609:AddCloseCallback; 614:Close; 689:IsClosed; 694:Run; 718:SetServiceAuditBackendForHTTP; 728:Context; 733:LoopContext; 887:IsKeyspaceGroupEnabled; 892:GetAddr; 897:GetClientScheme; 905:GetMemberInfo; 910:GetHandler; 915:GetEndpoints; 920:GetClient; 925:GetHTTPClient; 930:GetLeader; 935:GetServingUrls; 940:GetMember; 945:GetStorage; 950:GetGCStateManager; 955:GetHistoryHotRegionStorage; 961:SetStorage; 966:GetBasicCluster; 972:ResetFollowerRegionCache; 1120:GetPersistOptions; 1125:GetServiceMiddlewarePersistOptions; 1130:GetHBStreams; 1135:GetAllocator; 1140:GetTSOAllocator; 1145:GetKeyspaceManager; 1151:SetKeyspaceManager; 1156:GetKeyspaceGroupManager; 1161:GetMetaServiceGroupManager; 1167:SetKeyspaceGroupManager; 1172:GetMeteringWriter; 1177:Name; 1182:StartTimestamp; 1187:GetMembers; 1192:ReloadMembers; 1209:GetServiceMiddlewareConfig; 1218:GetConfig; 1232:GetKeyspaceConfig; 1237:SetKeyspaceConfig; 1242:SetKeyspaceConfigWithoutKeyspaceManagerUpdate; 1247:UpdateKeyspaceConfig; 1276:GetMicroserviceConfig; 1281:SetMicroserviceConfig; 1297:GetScheduleConfig; 1302:SetScheduleConfig; 1311:PatchScheduleConfig; 1322:SetScheduleConfigItem; 1372:GetReplicationConfig; 1377:SetReplicationConfig; 1471:GetAuditConfig; 1476:SetAuditConfig; 1492:UpdateRateLimitConfig; 1503:UpdateRateLimit; 1520:GetRateLimitConfig; 1525:SetRateLimitConfig; 1541:UpdateGRPCRateLimitConfig; 1552:UpdateGRPCRateLimit; 1569:GetGRPCRateLimitConfig; 1574:SetGRPCRateLimitConfig; 1590:GetPDServerConfig; 1595:SetPDServerConfig; 1628:SetLabelPropertyConfig; 1644:SetLabelProperty; 1663:DeleteLabelProperty; 1682:GetLabelProperty; 1687:SetClusterVersion; 1708:GetClusterVersion; 1713:GetTLSConfig; 1718:GetControllerConfig; 1724:GetRaftCluster; 1732:IsServiceIndependent; 1744:DirectlyGetRaftCluster; 1749:GetCluster; 1757:GetServerOption; 1762:GetMetaRegions; 1771:GetRegions; 1781:GetServiceLabels; 1789:IsGRPCServiceLabelExist; 1796:GetAPIAccessServiceLabel; 1809:AddServiceLabel; 1822:GetAuditBackend; 1827:GetServiceAuditBackendLabels; 1832:SetServiceAuditBackendLabels; 1837:GetServiceRateLimiter; 1842:IsInRateLimitAllowList; 1847:UpdateServiceRateLimiter; 1852:GetGRPCRateLimiter; 1857:UpdateGRPCServiceRateLimiter; 1862:GetClusterStatus; 1867:SetLogLevel; 1878:GetReplicationModeConfig; 1883:SetReplicationModeConfig; 1923:IsServing; 1928:AddServiceReadyCallback; 2278:ReplicateFileToMember; 2302:PersistFile; 2315:SaveTTLConfig; 2346:IsTTLConfigExist; 2360:MarkSnapshotRecovering; 2373:IsSnapshotRecovering; 2382:UnmarkSnapshotRecovering; 2390:MarkPitrRestoreMode; 2403:IsPitrRestoreMode; 2412:UnmarkPitrRestoreMode; 2421:GetServicePrimaryAddr; 2447:SetServicePrimaryAddr; 2453:DeleteServicePrimaryAddr; 2527:RecoverAllocID; 2532:GetExternalTS; 2541:SetExternalTS; 2563:GetMaxConcurrentTSOProxyStreamings; 2568:GetTSOProxyRecvFromClientTimeout; 2573:GetLease; 2578:GetTSOSaveInterval; 2583:GetTSOUpdatePhysicalInterval; 2588:GetMaxResetTSGap; 2594:SetClient; 2599:GetAffinityManager | github.com/tikv/pd/pkg/audit; github.com/tikv/pd/pkg/basicserver; github.com/tikv/pd/pkg/cgroup; github.com/tikv/pd/pkg/core; github.com/tikv/pd/pkg/encryption; github.com/tikv/pd/pkg/errs; github.com/tikv/pd/pkg/gc; github.com/tikv/pd/pkg/id; github.com/tikv/pd/pkg/keyspace; github.com/tikv/pd/pkg/keyspace/constant; github.com/tikv/pd/pkg/mcs/metastorage/server; github.com/tikv/pd/pkg/mcs/registry; github.com/tikv/pd/pkg/mcs/resourcemanager/server; github.com/tikv/pd/pkg/mcs/utils/constant; github.com/tikv/pd/pkg/member; github.com/tikv/pd/pkg/metering; github.com/tikv/pd/pkg/ratelimit; github.com/tikv/pd/pkg/replication; github.com/tikv/pd/pkg/schedule/affinity; github.com/tikv/pd/pkg/schedule/config; github.com/tikv/pd/pkg/schedule/hbstream; github.com/tikv/pd/pkg/schedule/placement; github.com/tikv/pd/pkg/storage; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/pkg/storage/kv; github.com/tikv/pd/pkg/syncer; github.com/tikv/pd/pkg/systimemon; github.com/tikv/pd/pkg/tso; github.com/tikv/pd/pkg/utils/apiutil; github.com/tikv/pd/pkg/utils/etcdutil; github.com/tikv/pd/pkg/utils/grpcutil; github.com/tikv/pd/pkg/utils/jsonutil; github.com/tikv/pd/pkg/utils/keypath; github.com/tikv/pd/pkg/utils/logutil; github.com/tikv/pd/pkg/utils/syncutil; github.com/tikv/pd/pkg/utils/tsoutil; github.com/tikv/pd/pkg/utils/typeutil; github.com/tikv/pd/pkg/versioninfo; github.com/tikv/pd/pkg/versioninfo/kerneltype; github.com/tikv/pd/server/cluster; github.com/tikv/pd/server/config; github.com/tikv/pd/pkg/mcs/resourcemanager/server/apis/v1; github.com/tikv/pd/pkg/mcs/router/server/apis/v1; github.com/tikv/pd/pkg/mcs/tso/server/apis/v1 |
| [`tests/rust-compat/fixtures/different_service.rs`](../../../tests/rust-compat/fixtures/different_service.rs) | 100,101 | 无匹配导出声明；内部实现/注册仍纳入来源 | use serde_json::json;; use std::{ |
| [`tests/rust-compat/harness.rs`](../../../tests/rust-compat/harness.rs) | 4,98,99,100,101 | 14:pub(crate) mod fixture;; 16:pub struct Server {; 24:pub fn start_rust(binary: &Path) -> io::Result<Self> {; 32:pub fn stop(&mut self) -> Result<(), String> {; 52:pub fn start(binary: &Path) -> io::Result<Self> {; 55:pub fn start_with_args(binary: &Path, extra: &[&str]) -> io::Result<Self> {; 98:pub fn ready(&mut self) -> Result<(), String> {; 101:pub fn ready_with_timeout(&mut self, timeout: Duration) -> Result<(), String> {; 123:pub fn logs(&self) -> String {; 126:pub fn get(&self, path: &str) -> Result<(u16, Value), String> {; 150:pub fn sample(&self) -> Result<Value, String> {; 184:pub fn run(mode: &str, go: &Path, rust: Option<&Path>) -> Result<(), String> {; 215:pub fn minimal_sample(server: &Server) -> Result<Value, String> {; 267:pub fn run_minimal(mode: &str, go: &Path, rust: Option<&Path>) -> Result<(), String> { | use crate::normalize::{Rules, differences};; use serde_json::{Value, json};; use std::{ |
| [`tests/rust-compat/lib.rs`](../../../tests/rust-compat/lib.rs) | 100,101 | 3:pub mod harness;; 4:pub mod normalize; | 见文件imports/use及就近Cargo.toml（无匹配内部依赖行） |
| [`tests/rust-compat/normalize.rs`](../../../tests/rust-compat/normalize.rs) | 100,101 | 7:pub struct Rules(pub Vec<(Value, Value)>);; 9:pub fn apply(&self, value: &Value) -> Value {; 37:pub fn differences(left: &Value, right: &Value) -> Vec<String> {; 65:pub fn metrics(text: &str) -> Vec<String> {; 78:pub fn tso(bytes: &[u8], physical_origin: i64) -> Result<Value, prost::DecodeError> {; 90:pub fn minimal_member(value: &Value) -> Result<Value, String> { | use serde_json::Value;; use prost::Message; |
| [`tests/rust-compat/schedule_foundation.rs`](../../../tests/rust-compat/schedule_foundation.rs) | 100,101 | 16:pub async fn contract(store: Arc<dyn MetaStore>, seed: bool) { | use astersql_pd_core::{; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_keyutil::KeyRange;; use astersql_pd_schedule_affinity::{manager::Manager, ranges::GroupKeyRanges};; use astersql_pd_schedule_config::ProviderSnapshot;; use astersql_pd_schedule_labeler::RegionLabeler;; use astersql_pd_schedule_placement::{Rule, RuleManager, fit_region};; use astersql_pd_storage_endpoint::StorageEndpoint;; use astersql_pd_storage_kv::{Base, MetaStore};; use std::sync::Arc; |
| [`tests/rust-compat/state_oracle.rs`](../../../tests/rust-compat/state_oracle.rs) | 100,101 | 无匹配导出声明；内部实现/注册仍纳入来源 | use astersql_pd_proto::{; use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};; use prost::Message;; use serde_json::{Value, json};; use std::time::Duration;; use tokio::time::{sleep, timeout}; |
| [`tests/rust-compat/state_regression.rs`](../../../tests/rust-compat/state_regression.rs) | 100,101 | 32:pub fn hex(bytes: &[u8]) -> String {; 35:pub fn unhex(v: &Value) -> Vec<u8> {; 42:pub fn fixture() -> Value {; 55:pub fn compare(expected: &Value, actual: &Value) -> Result<(), String> {; 101:pub fn normalize(mut s: Value) -> Value {; 343:pub async fn replay( | use astersql_pd_cluster::*;; use astersql_pd_core::*;; use astersql_pd_keypath::ClusterPaths;; use astersql_pd_proto::{metapb, pdpb};; use astersql_pd_ratelimit::ConcurrentRunner;; use astersql_pd_schedule_config::ProviderSnapshot;; use astersql_pd_schedule_placement::RuleManager;; use astersql_pd_server::{; use astersql_pd_statistics::{HotStat, LabelStatistics, RegionStatistics, get_hot_status};; use astersql_pd_statistics_utils::RWType;; use astersql_pd_storage_endpoint::StorageEndpoint;; use astersql_pd_storage_kv::MetaStore;; use astersql_pd_syncer::*;; use prometheus::Registry;; use prost::Message;; use serde_json::{Value, json};; use std::{; use tokio::{runtime::Handle, time::timeout};; use tokio_util::sync::CancellationToken; |
| [`tests/server/api/utils.go`](../../../tests/server/api/utils.go) | 99 | 无匹配导出声明；内部实现/注册仍纳入来源 | github.com/tikv/pd/pkg/utils/testutil; github.com/tikv/pd/tests |
| [`tests/server/apiv2/handlers/testutil.go`](../../../tests/server/apiv2/handlers/testutil.go) | 99 | 83:MustCreateKeyspace; 101:MustCreateKeyspaceByID; 189:MustLoadKeyspaceGroups; 223:MustLoadKeyspaceGroupByID; 236:TryLoadKeyspaceGroupByID; 254:MustCreateKeyspaceGroup; 260:FailCreateKeyspaceGroupWithCode; 266:MustDeleteKeyspaceGroup; 278:MustSplitKeyspaceGroup; 293:MustFinishSplitKeyspaceGroup; 332:MustMergeKeyspaceGroup; 420:MustRemoveKeyspacesFromGroup; 440:FailRemoveKeyspacesFromGroupWithCode | github.com/tikv/pd/pkg/keyspace; github.com/tikv/pd/pkg/storage/endpoint; github.com/tikv/pd/pkg/utils/testutil; github.com/tikv/pd/server/apiv2/handlers; github.com/tikv/pd/tests |

## worker、注入与持久化定位

下表是直接服务生产文件逐行扫描结果；扫描保留Inject、InjectCall、Enable/Disable、Return/Eval及其它`failpoint.`形式，也保留Tokio spawn和Go协程启动。不把返回辅助宏误算为独立故障点。注释匹配同样保留供人工复核；多行调用需打开原文阅读。
持久化列定位keypath调用、明确路径文字及Load/Save/Txn/Watch/Put/Delete/Remove等操作；它是调用站点索引，精确存储事务/codec应在所属任务连同依赖实现和keypath定义核验。

| 类别 | 来源行 | owner | 原文 |
| --- | --- | --- | --- |
| worker | `cmd/pd-server/main.go:303` | 3,90,91 | go func() { |
| persistence | `pkg/runtime/node.rs:21` | 1,3,5,7,83,84,85,86,87,89,93,94,95,98 | let key = b"/pd/cluster_id"; |
| persistence | `pkg/runtime/node.rs:30` | 1,3,5,7,83,84,85,86,87,89,93,94,95,98 | .transact( |
| worker | `pkg/runtime/node.rs:116` | 1,3,5,7,83,84,85,86,87,89,93,94,95,98 | .spawn(initialize_node(store, config, listener, registry, cancel)) |
| worker | `pkg/runtime/node.rs:183` | 1,3,5,7,83,84,85,86,87,89,93,94,95,98 | let worker = tokio::spawn(async move { |
| persistence | `pkg/server/http.rs:33` | 3,38,87 | .route("/pd/api/v1/health", get(health)) |
| persistence | `pkg/server/http.rs:34` | 3,38,87 | .route("/pd/api/v1/members", get(members)) |
| worker | `pkg/server/sync_transport.rs:188` | 3,11 | tasks.push(self.runtime.spawn(async move { |
| worker | `pkg/tso/allocator.go:110` | 35,36,75,76 | go a.allocatorUpdater() |
| failpoint | `pkg/tso/allocator.go:121` | 35,36,75,76 | failpoint.Inject("fastUpdatePhysicalInterval", func() { |
| worker | `pkg/tso/allocator.go:258` | 35,36,75,76 | go a.primaryElectionLoop() |
| persistence | `pkg/tso/allocator.go:283` | 35,36,75,76 | primary.Watch(a.ctx) |
| persistence | `pkg/tso/allocator.go:289` | 35,36,75,76 | expectedPrimary, err := mcsutils.GetExpectedPrimaryFlag(a.member.Client(), &keypath.MsParam{ |
| persistence | `pkg/tso/allocator.go:321` | 35,36,75,76 | msParam := &keypath.MsParam{ |
| persistence | `pkg/tso/allocator.go:361` | 35,36,75,76 | mcsutils.DeleteExpectedPrimaryFlag(a.member.Client(), msParam, expectedPrimary) |
| worker | `pkg/tso/keyspace_group_manager.go:141` | 35,36,75,76 | go func(allocator *Allocator) { |
| persistence | `pkg/tso/keyspace_group_manager.go:439` | 35,36,75,76 | kgm.compiledKGMembershipIDRegexp = keypath.GetCompiledKeyspaceGroupIDRegexp() |
| worker | `pkg/tso/keyspace_group_manager.go:458` | 35,36,75,76 | go kgm.primaryPriorityCheckLoop() |
| worker | `pkg/tso/keyspace_group_manager.go:459` | 35,36,75,76 | go kgm.groupSplitPatroller() |
| worker | `pkg/tso/keyspace_group_manager.go:460` | 35,36,75,76 | go kgm.deletedGroupCleaner() |
| worker | `pkg/tso/keyspace_group_manager.go:461` | 35,36,75,76 | go kgm.keyspaceGroupMetricsSyncer() |
| persistence | `pkg/tso/keyspace_group_manager.go:507` | 35,36,75,76 | kgm.tsoNodes.Delete(serviceAddr) |
| persistence | `pkg/tso/keyspace_group_manager.go:519` | 35,36,75,76 | keypath.ServicePath(mcs.TSOServiceName), |
| failpoint | `pkg/tso/keyspace_group_manager.go:548` | 35,36,75,76 | failpoint.Inject("SkipKeyspaceWatch", func(val failpoint.Value) { |
| failpoint | `pkg/tso/keyspace_group_manager.go:551` | 35,36,75,76 | failpoint.Return(nil) |
| failpoint | `pkg/tso/keyspace_group_manager.go:575` | 35,36,75,76 | failpoint.Inject("SkipKeyspaceWatch", func(val failpoint.Value) { |
| failpoint | `pkg/tso/keyspace_group_manager.go:578` | 35,36,75,76 | failpoint.Return(nil) |
| persistence | `pkg/tso/keyspace_group_manager.go:598` | 35,36,75,76 | strings.TrimSuffix(keypath.KeyspaceGroupIDPrefix(), "/"), |
| failpoint | `pkg/tso/keyspace_group_manager.go:640` | 35,36,75,76 | failpoint.Inject("fastPrimaryPriorityCheck", func() { |
| worker | `pkg/tso/keyspace_group_manager.go:760` | 35,36,75,76 | go kgm.mergingChecker(ctx, group.ID, group.MergeState.MergeList) |
| persistence | `pkg/tso/keyspace_group_manager.go:766` | 35,36,75,76 | if cancel, loaded := kgm.mergeCheckerCancelMap.LoadAndDelete(group.ID); loaded && cancel != nil { |
| failpoint | `pkg/tso/keyspace_group_manager.go:772` | 35,36,75,76 | failpoint.Inject("delayBeforeCheckingElectionMember", func() { |
| persistence | `pkg/tso/keyspace_group_manager.go:798` | 35,36,75,76 | participant := member.NewParticipant(kgm.etcdClient, keypath.MsParam{ |
| persistence | `pkg/tso/keyspace_group_manager.go:1295` | 35,36,75,76 | const keyspaceGroupsAPIPrefix = "/pd/api/v2/tso/keyspace-groups" |
| failpoint | `pkg/tso/keyspace_group_manager.go:1331` | 35,36,75,76 | failpoint.Inject("finishKeyspaceGroupPerEndpointTimeout", func(val failpoint.Value) { |
| persistence | `pkg/tso/keyspace_group_manager.go:1507` | 35,36,75,76 | electionPath := keypath.ElectionPath(&keypath.MsParam{ |
| persistence | `pkg/tso/keyspace_group_manager.go:1511` | 35,36,75,76 | val, err := kgm.storage.Load(electionPath) |
| persistence | `pkg/tso/keyspace_group_manager.go:1540` | 35,36,75,76 | ts, err := kgm.storage.LoadTimestamp(id) |
| failpoint | `pkg/tso/keyspace_group_manager.go:1605` | 35,36,75,76 | failpoint.Inject("fastGroupSplitPatroller", func() { |
| failpoint | `pkg/tso/keyspace_group_manager.go:1681` | 35,36,75,76 | failpoint.Inject("fastDeletedGroupCleaner", func() { |
| persistence | `pkg/tso/keyspace_group_manager.go:1717` | 35,36,75,76 | err := kgm.storage.DeleteTimestamp(kgm.ctx, groupID) |
| persistence | `pkg/tso/metrics.go:219` | 35,36,75,76 | if g, ok := keyspaceGroupKeyspaceCountGaugeCache.Load(key); ok { |
| persistence | `pkg/tso/metrics.go:224` | 35,36,75,76 | if actual, loaded := keyspaceGroupKeyspaceCountGaugeCache.LoadOrStore(key, gauge); loaded { |
| persistence | `pkg/tso/metrics.go:232` | 35,36,75,76 | keyspaceGroupKeyspaceCountGauge.DeleteLabelValues(strconv.FormatUint(uint64(groupID), 10)) |
| persistence | `pkg/tso/metrics.go:233` | 35,36,75,76 | keyspaceGroupKeyspaceCountGaugeCache.Delete(groupID) |
| persistence | `pkg/tso/oracle.rs:98` | 35,36,75,76 | key: format!("/pd/{}/timestamp", leader.cluster_id()).into_bytes(), |
| persistence | `pkg/tso/oracle.rs:155` | 35,36,75,76 | .transact( |
| persistence | `pkg/tso/oracle.rs:184` | 35,36,75,76 | .transact( |
| persistence | `pkg/tso/tso.go:90` | 35,36,75,76 | return t.storage.SaveTimestamp(ctx, t.keyspaceGroupID, ts, t.member.GetLeadership()) |
| failpoint | `pkg/tso/tso.go:152` | 35,36,75,76 | failpoint.InjectCall("beforeGenerateTSO", physical, &t.tsoMux.logical, count) |
| persistence | `pkg/tso/tso.go:159` | 35,36,75,76 | last := t.lastSavedTime.Load() |
| failpoint | `pkg/tso/tso.go:170` | 35,36,75,76 | failpoint.Inject("delaySyncTimestamp", func() { |
| persistence | `pkg/tso/tso.go:174` | 35,36,75,76 | last, err := t.storage.LoadTimestamp(t.keyspaceGroupID) |
| failpoint | `pkg/tso/tso.go:198` | 35,36,75,76 | failpoint.Inject("fallBackSync", func() { |
| failpoint | `pkg/tso/tso.go:201` | 35,36,75,76 | failpoint.Inject("systemTimeSlow", func() { |
| failpoint | `pkg/tso/tso.go:214` | 35,36,75,76 | failpoint.Inject("failedToSaveTimestamp", func() { |
| failpoint | `pkg/tso/tso.go:215` | 35,36,75,76 | failpoint.Return(errs.ErrEtcdTxnInternal) |
| failpoint | `pkg/tso/tso.go:347` | 35,36,75,76 | failpoint.Inject("failedToUpdateTimestamp", func() { |
| failpoint | `pkg/tso/tso.go:348` | 35,36,75,76 | failpoint.Return(false, errs.ErrEtcdTxnInternal) |
| failpoint | `pkg/tso/tso.go:351` | 35,36,75,76 | failpoint.Inject("fallBackUpdate", func() { |
| failpoint | `pkg/tso/tso.go:354` | 35,36,75,76 | failpoint.Inject("systemTimeSlow", func() { |
| failpoint | `pkg/tso/tso.go:455` | 35,36,75,76 | failpoint.InjectCall("onLogicalOverflow", count) |
| persistence | `pkg/utils/apiutil/apiutil.go:85` | 38,41,82 | _ = godotenv.Load() |
| persistence | `pkg/utils/apiutil/apiutil.go:440` | 38,41,82 | CorePath = "/pd/api/v1" |
| persistence | `pkg/utils/apiutil/apiutil.go:442` | 38,41,82 | ExtensionsPath = "/pd/apis" |
| persistence | `pkg/utils/apiutil/serverapi/middleware.go:191` | 40,41,82 | "/pd/api/v1/admin/log", |
| persistence | `pkg/utils/apiutil/serverapi/middleware.go:192` | 40,41,82 | "/pd/api/v1/status", |
| persistence | `pkg/utils/apiutil/serverapi/middleware.go:193` | 40,41,82 | "/pd/api/v1/health", |
| persistence | `pkg/utils/apiutil/serverapi/middleware.go:194` | 40,41,82 | "/pd/api/v1/ping", |
| persistence | `pkg/utils/apiutil/serverapi/middleware.go:195` | 40,41,82 | "/pd/api/v1/version", |
| persistence | `pkg/utils/apiutil/serverapi/middleware.go:196` | 40,41,82 | "/pd/api/v1/debug/pprof", |
| persistence | `pkg/utils/apiutil/serverapi/middleware.go:203` | 40,41,82 | if r.URL.Path == "/pd/api/v1/config" && r.Method == http.MethodGet { |
| persistence | `pkg/utils/grpcutil/cluster.go:270` | 42,82,83,84,85 | clusterID := keypath.ClusterID() |
| persistence | `pkg/utils/grpcutil/cluster.go:288` | 42,82,83,84,85 | ClusterId: keypath.ClusterID(), |
| persistence | `pkg/utils/keypath/absolute_key_path.go:37` | 6,20,32,33,34,35,48,49,50,69,70,71 | ClusterIDPath = "/pd/cluster_id" // "/pd/cluster_id" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:39` | 6,20,32,33,34,35,48,49,50,69,70,71 | leaderPathFormat            = "/pd/%d/leader"                             // "/pd/{cluster_id}/leader" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:40` | 6,20,32,33,34,35,48,49,50,69,70,71 | allocIDPathFormat           = "/pd/%d/alloc_id"                           // "/pd/{cluster_id}/alloc_id" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:41` | 6,20,32,33,34,35,48,49,50,69,70,71 | keyspaceAllocIDPathFormat   = "/pd/%d/keyspaces/alloc_id"                 // "/pd/{cluster_id}/keyspaces/alloc_id" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:42` | 6,20,32,33,34,35,48,49,50,69,70,71 | configPathFormat            = "/pd/%d/config"                             // "/pd/{cluster_id}/config" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:43` | 6,20,32,33,34,35,48,49,50,69,70,71 | schedulerConfigPathFormat   = "/pd/%d/scheduler_config/%s"                // "/pd/{cluster_id}/scheduler_config/{scheduler_name}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:44` | 6,20,32,33,34,35,48,49,50,69,70,71 | storeLeaderWeightPathFormat = "/pd/%d/schedule/store_weight/%020d/leader" // "/pd/{cluster_id}/schedule/store_weight/{store_id}/leader" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:45` | 6,20,32,33,34,35,48,49,50,69,70,71 | storeRegionWeightPathFormat = "/pd/%d/schedule/store_weight/%020d/region" // "/pd/{cluster_id}/schedule/store_weight/{store_id}/region" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:47` | 6,20,32,33,34,35,48,49,50,69,70,71 | serviceMiddlewarePathFormat   = "/pd/%d/service_middleware"                  // "/pd/{cluster_id}/service_middleware" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:48` | 6,20,32,33,34,35,48,49,50,69,70,71 | replicationModePathFormat     = "/pd/%d/replication_mode/%s"                 // "/pd/{cluster_id}/replication_mode/{mode}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:49` | 6,20,32,33,34,35,48,49,50,69,70,71 | recoveringMarkPathFormat      = "/pd/%d/cluster/markers/snapshot-recovering" // "/pd/{cluster_id}/cluster/markers/snapshot-recovering" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:50` | 6,20,32,33,34,35,48,49,50,69,70,71 | pitrRestoreModeMarkPathFormat = "/pd/%d/cluster/markers/pitr-restore-mode"   // "/pd/{cluster_id}/cluster/markers/pitr-restore-mode" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:52` | 6,20,32,33,34,35,48,49,50,69,70,71 | memberBinaryDeployPathFormat   = "/pd/%d/member/%d/deploy_path"     // "/pd/{cluster_id}/member/{member_id}/deploy_path" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:53` | 6,20,32,33,34,35,48,49,50,69,70,71 | memberGitHashPath              = "/pd/%d/member/%d/git_hash"        // "/pd/{cluster_id}/member/{member_id}/git_hash" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:54` | 6,20,32,33,34,35,48,49,50,69,70,71 | memberBinaryVersionPathFormat  = "/pd/%d/member/%d/binary_version"  // "/pd/{cluster_id}/member/{member_id}/binary_version" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:55` | 6,20,32,33,34,35,48,49,50,69,70,71 | memberLeaderPriorityPathFormat = "/pd/%d/member/%d/leader_priority" // "/pd/{cluster_id}/member/{member_id}/leader_priority" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:57` | 6,20,32,33,34,35,48,49,50,69,70,71 | rulePathFormat = "/pd/%d/rules/%s" // "/pd/{cluster_id}/rules/{rule_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:59` | 6,20,32,33,34,35,48,49,50,69,70,71 | ruleCommonPrefixFormat  = "/pd/%d/rule"            // "/pd/{cluster_id}/rule" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:60` | 6,20,32,33,34,35,48,49,50,69,70,71 | ruleGroupPathFormat     = "/pd/%d/rule_group/%s"   // "/pd/{cluster_id}/rule_group/{group_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:61` | 6,20,32,33,34,35,48,49,50,69,70,71 | regionLabelPathFormat   = "/pd/%d/region_label/%s" // "/pd/{cluster_id}/region_label/{label_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:62` | 6,20,32,33,34,35,48,49,50,69,70,71 | regionLabelPrefixFormat = "/pd/%d/region_label/"   // "/pd/{cluster_id}/region_label/" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:65` | 6,20,32,33,34,35,48,49,50,69,70,71 | maintenanceTaskPathFormat = "/pd/%d/maintenance/%s" // "/pd/{cluster_id}/maintenance/{task_type}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:70` | 6,20,32,33,34,35,48,49,50,69,70,71 | gcStateRevisionPathFormat            = "/pd/%d/gc/gc_state_revision"         // "/pd/{cluster_id}/gc/gc_state_revision" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:71` | 6,20,32,33,34,35,48,49,50,69,70,71 | unifiedGCSafePointPathFormat         = "/pd/%d/gc/safe_point"                // "/pd/{cluster_id}/gc/safe_point" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:72` | 6,20,32,33,34,35,48,49,50,69,70,71 | keyspaceLevelGCSafePointPrefixFormat = "/pd/%d/keyspaces/gc_safe_point/"     // "/pd/{cluster_id}/keyspaces/gc_safe_point/" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:73` | 6,20,32,33,34,35,48,49,50,69,70,71 | keyspaceLevelGCSafePointPathFormat   = "/pd/%d/keyspaces/gc_safe_point/%08d" // "/pd/{cluster_id}/keyspaces/gc_safe_point/{keyspace_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:78` | 6,20,32,33,34,35,48,49,50,69,70,71 | unifiedGCBarrierPathFormat        = "/pd/%d/gc/safe_point/service/%s"                         // "/pd/{cluster_id}/gc/safe_point/service/{barrier_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:79` | 6,20,32,33,34,35,48,49,50,69,70,71 | keyspaceLevelGCBarrierPathFormat  = "/pd/%d/keyspaces/service_safe_point/%08d/%s"             // "/pd/{cluster_id}/keyspaces/service_safe_point/{keyspace_id}/{barrier_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:80` | 6,20,32,33,34,35,48,49,50,69,70,71 | globalGCBarrierPathFormat         = "/pd/%d/gc/global_gc_barriers/%s"                         // "/pd/{cluster_id}/gc/global_gc_barriers/{barrier_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:81` | 6,20,32,33,34,35,48,49,50,69,70,71 | globalGCBarrierPathPrefix         = "/pd/%d/gc/global_gc_barriers/" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:88` | 6,20,32,33,34,35,48,49,50,69,70,71 | clusterPathFormat              = "/pd/%d/raft"                            // "/pd/{cluster_id}/raft" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:89` | 6,20,32,33,34,35,48,49,50,69,70,71 | clusterBootstrapTimePathFormat = "/pd/%d/raft/status/raft_bootstrap_time" // "/pd/{cluster_id}/raft/status/raft_bootstrap_time" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:90` | 6,20,32,33,34,35,48,49,50,69,70,71 | storePathPrefixFormat          = "/pd/%d/raft/s/"                         // "/pd/{cluster_id}/raft/s/" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:91` | 6,20,32,33,34,35,48,49,50,69,70,71 | storePathFormat                = "/pd/%d/raft/s/%020d"                    // "/pd/{cluster_id}/raft/s/{store_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:92` | 6,20,32,33,34,35,48,49,50,69,70,71 | minResolvedTSPathFormat        = "/pd/%d/raft/min_resolved_ts"            // "/pd/{cluster_id}/raft/min_resolved_ts" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:93` | 6,20,32,33,34,35,48,49,50,69,70,71 | externalTimestampPathFormat    = "/pd/%d/raft/external_timestamp"         // "/pd/{cluster_id}/raft/external_timestamp" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:95` | 6,20,32,33,34,35,48,49,50,69,70,71 | keyspaceMetaPrefixFormat    = "/pd/%d/keyspaces/meta/"                     // "/pd/{cluster_id}/keyspaces/meta/" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:96` | 6,20,32,33,34,35,48,49,50,69,70,71 | keyspaceMetaPathFormat      = "/pd/%d/keyspaces/meta/%08d"                 // "/pd/{cluster_id}/keyspaces/meta/{keyspace_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:97` | 6,20,32,33,34,35,48,49,50,69,70,71 | keyspaceIDPathFormat        = "/pd/%d/keyspaces/id/%s"                     // "/pd/{cluster_id}/keyspaces/id/{keyspace_name}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:98` | 6,20,32,33,34,35,48,49,50,69,70,71 | keyspaceGroupIDPrefixFormat = "/pd/%d/tso/keyspace_groups/membership/"     // "/pd/{cluster_id}/tso/keyspace_groups/membership/" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:99` | 6,20,32,33,34,35,48,49,50,69,70,71 | keyspaceGroupIDPathFormat   = "/pd/%d/tso/keyspace_groups/membership/%05d" // "/pd/{cluster_id}/tso/keyspace_groups/membership/{group_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:133` | 6,20,32,33,34,35,48,49,50,69,70,71 | timestampPathFormat   = "/pd/%d/timestamp"              // "/pd/{cluster_id}/timestamp" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:136` | 6,20,32,33,34,35,48,49,50,69,70,71 | affinityGroupPathFormat = "/pd/%d/affinity_groups/%s" // "/pd/{cluster_id}/affinity_groups/{group_id}" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:139` | 6,20,32,33,34,35,48,49,50,69,70,71 | metaServiceGroupStatusFormat = "/pd/%d/meta_service_groups/%s/status" // "/pd/{cluster_id}/meta_service_groups/{group_id}/status" |
| persistence | `pkg/utils/keypath/absolute_key_path.go:290` | 6,20,32,33,34,35,48,49,50,69,70,71 | buf.WriteString("/pd/") |
| persistence | `pkg/utils/keypath/affinity.go:22` | 6,20,32,33,34,35,48,49,50,69,70,71 | // Its format is: "/pd/{cluster_id}/affinity_groups/{group_id}" |
| persistence | `pkg/utils/keypath/affinity.go:28` | 6,20,32,33,34,35,48,49,50,69,70,71 | // Its format is: "/pd/{cluster_id}/affinity_groups/" |
| persistence | `pkg/utils/keypath/cluster_id.go:25` | 6,20,32,33,34,35,48,49,50,69,70,71 | id := clusterID.Load() |
| persistence | `pkg/utils/keypath/cluster_paths.rs:5` | 6,20,32,33,34,35,48,49,50,69,70,71 | pub const CLUSTER_ID_PATH: &str = "/pd/cluster_id"; |
| persistence | `pkg/utils/keypath/cluster_paths.rs:43` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/raft", self.cluster_id) |
| persistence | `pkg/utils/keypath/cluster_paths.rs:51` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/alloc_id", self.cluster_id) |
| persistence | `pkg/utils/keypath/cluster_paths.rs:55` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/keyspaces/alloc_id", self.cluster_id) |
| persistence | `pkg/utils/keypath/gc_paths.rs:15` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/gc/gc_state_revision", self.cluster_id()) |
| persistence | `pkg/utils/keypath/gc_paths.rs:20` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/gc/safe_point", self.cluster_id()) |
| persistence | `pkg/utils/keypath/gc_paths.rs:41` | 6,20,32,33,34,35,48,49,50,69,70,71 | "/pd/{}/gc/safe_point/service/{barrier_id}", |
| persistence | `pkg/utils/keypath/gc_paths.rs:55` | 6,20,32,33,34,35,48,49,50,69,70,71 | "/pd/{}/gc/global_gc_barriers/{barrier_id}", |
| persistence | `pkg/utils/keypath/gc_paths.rs:77` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/keyspaces/gc_safe_point/", self.cluster_id()) |
| persistence | `pkg/utils/keypath/gc_paths.rs:86` | 6,20,32,33,34,35,48,49,50,69,70,71 | "/pd/{}/keyspaces/service_safe_point/{keyspace_id:08}/{service_id}", |
| persistence | `pkg/utils/keypath/keyspace_time_paths.rs:8` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/keyspaces/meta/", self.cluster_id()) |
| persistence | `pkg/utils/keypath/keyspace_time_paths.rs:16` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/keyspaces/id/{name}", self.cluster_id()) |
| persistence | `pkg/utils/keypath/keyspace_time_paths.rs:20` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/tso/keyspace_groups/membership/", self.cluster_id()) |
| persistence | `pkg/utils/keypath/keyspace_time_paths.rs:29` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/timestamp", self.cluster_id()) |
| persistence | `pkg/utils/keypath/keyspace_time_paths.rs:45` | 6,20,32,33,34,35,48,49,50,69,70,71 | "/pd/{}/meta_service_groups/{group_id}/status", |
| persistence | `pkg/utils/keypath/member_config_paths.rs:45` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/config", self.cluster_id()) |
| persistence | `pkg/utils/keypath/member_config_paths.rs:53` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/scheduler_config/{name}", self.cluster_id()) |
| persistence | `pkg/utils/keypath/member_config_paths.rs:57` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/service_middleware", self.cluster_id()) |
| persistence | `pkg/utils/keypath/member_config_paths.rs:61` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/replication_mode/{mode}", self.cluster_id()) |
| persistence | `pkg/utils/keypath/member_config_paths.rs:66` | 6,20,32,33,34,35,48,49,50,69,70,71 | "/pd/{}/schedule/store_weight/{id:020}/leader", |
| persistence | `pkg/utils/keypath/member_config_paths.rs:73` | 6,20,32,33,34,35,48,49,50,69,70,71 | "/pd/{}/schedule/store_weight/{id:020}/region", |
| persistence | `pkg/utils/keypath/member_config_paths.rs:79` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/member/{id}/deploy_path", self.cluster_id()) |
| persistence | `pkg/utils/keypath/member_config_paths.rs:83` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/member/{id}/git_hash", self.cluster_id()) |
| persistence | `pkg/utils/keypath/member_config_paths.rs:87` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/member/{id}/binary_version", self.cluster_id()) |
| persistence | `pkg/utils/keypath/member_config_paths.rs:91` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/member/{id}/leader_priority", self.cluster_id()) |
| persistence | `pkg/utils/keypath/member_config_paths.rs:97` | 6,20,32,33,34,35,48,49,50,69,70,71 | None => format!("/pd/{}/leader", self.cluster_id()), |
| persistence | `pkg/utils/keypath/member_config_paths.rs:98` | 6,20,32,33,34,35,48,49,50,69,70,71 | Some(p) if p.service_name.is_empty() => format!("/pd/{}/leader", self.cluster_id()), |
| persistence | `pkg/utils/keypath/registry_marker_paths.rs:17` | 6,20,32,33,34,35,48,49,50,69,70,71 | "/pd/{}/cluster/markers/snapshot-recovering", |
| persistence | `pkg/utils/keypath/registry_marker_paths.rs:24` | 6,20,32,33,34,35,48,49,50,69,70,71 | "/pd/{}/cluster/markers/pitr-restore-mode", |
| persistence | `pkg/utils/keypath/registry_marker_paths.rs:30` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/maintenance/{task_type}", self.cluster_id()) |
| persistence | `pkg/utils/keypath/rule_resource_paths.rs:8` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/rules/{name}", self.cluster_id()) |
| persistence | `pkg/utils/keypath/rule_resource_paths.rs:12` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/rule_group/{name}", self.cluster_id()) |
| persistence | `pkg/utils/keypath/rule_resource_paths.rs:16` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/region_label/{name}", self.cluster_id()) |
| persistence | `pkg/utils/keypath/rule_resource_paths.rs:20` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/affinity_groups/{name}", self.cluster_id()) |
| persistence | `pkg/utils/keypath/rule_resource_paths.rs:40` | 6,20,32,33,34,35,48,49,50,69,70,71 | format!("/pd/{}/rule", self.cluster_id()) |
| persistence | `pkg/utils/logutil/log.go:175` | 92 | return curRedactType.Load().(RedactInfoLogType) |
| worker | `pkg/utils/metricutil/metricutil.go:105` | 86 | go prometheusPushClient(ctx, cfg.PushJob, cfg.PushAddress, interval) |
| persistence | `server/api/admin.go:81` | 73,74,75,76 | rc.RemoveRegionIfExist(regionID) |
| persistence | `server/api/admin.go:117` | 73,74,75,76 | if err = rc.GetStorage().DeleteRegion(targetRegion.GetMeta()); err != nil { |
| persistence | `server/api/admin.go:126` | 73,74,75,76 | rc.RemoveRegionIfExist(regionID) |
| persistence | `server/api/config.go:155` | 47,48,49,50 | err = h.svr.SaveTTLConfig(conf, time.Duration(ttlSec)*time.Second) |
| worker | `server/api/config.go:265` | 47,48,49,50 | // Meta-service group changes must go through the same safe update path as the |
| failpoint | `server/api/config.go:496` | 47,48,49,50 | failpoint.Inject("getReplicationConfigFailed", func(v failpoint.Value) { |
| persistence | `server/api/config.go:571` | 47,48,49,50 | err = h.svr.DeleteLabelProperty(input["type"], input["label-key"], input["label-value"]) |
| persistence | `server/api/member.go:75` | 44,45 | req := &pdpb.GetMembersRequest{Header: &pdpb.RequestHeader{ClusterId: keypath.ClusterID()}} |
| persistence | `server/api/member.go:148` | 44,45 | err = h.svr.GetMember().DeleteMemberLeaderPriority(id) |
| persistence | `server/api/member.go:155` | 44,45 | _, err = etcdutil.RemoveEtcdMember(client, id) |
| persistence | `server/api/member.go:182` | 44,45 | err = h.svr.GetMember().DeleteMemberLeaderPriority(id) |
| persistence | `server/api/member.go:189` | 44,45 | _, err = etcdutil.RemoveEtcdMember(client, id) |
| persistence | `server/api/metric.go:156` | 102 | request.Out.URL.Path = strings.Replace(request.In.URL.Path, "/pd/api/v1/metric", "/api/v1", 1) |
| failpoint | `server/api/middleware.go:72` | 38,41,79,80,81 | failpoint.Inject("addRequestInfoMiddleware", func() { |
| persistence | `server/api/operator.go:118` | 62,63 | if err := h.RemoveOperators(); err != nil { |
| persistence | `server/api/operator.go:175` | 62,63 | if err = h.RemoveOperator(regionID); err != nil { |
| failpoint | `server/api/region.go:80` | 55,56,57,58,59,67 | failpoint.Inject("RejectGetRegionByIDWhenAccessLeader", func() { |
| failpoint | `server/api/region.go:182` | 55,56,57,58,59,67 | failpoint.Inject("slowRequest", func() { |
| persistence | `server/api/region.go:307` | 55,56,57,58,59,67 | if _, err := keyspaceManager.LoadKeyspaceByID(keyspaceID); err != nil { |
| persistence | `server/api/region_label.go:149` | 70 | err = cluster.GetRegionLabeler().DeleteLabelRule(id) |
| failpoint | `server/api/router.go:393` | 2,81,101 | new(failpoint.HttpHandler).ServeHTTP(w, r) |
| persistence | `server/api/rule.go:318` | 68,69 | if err := manager.DeleteRule(group, id); err != nil { |
| persistence | `server/api/rule.go:423` | 68,69 | err := manager.DeleteRuleGroup(id) |
| persistence | `server/api/rule.go:534` | 68,69 | if err := manager.DeleteGroupBundle(group, regex); err != nil { |
| persistence | `server/api/scheduler.go:374` | 64,65 | if err := h.RemoveScheduler(name); err != nil { |
| failpoint | `server/api/server.go:181` | 102 | failpoint.Inject("enableAddServerNameHeader", func() { |
| persistence | `server/api/service_gc_safepoint.go:73` | 102 | if equivalentSsp.ServiceID == keypath.GCWorkerServiceSafePointID { |
| persistence | `server/api/service_gc_safepoint.go:83` | 102 | ServiceID:  keypath.GCWorkerServiceSafePointID, |
| persistence | `server/api/store.go:106` | 51,52,53,54 | err := rc.RemoveStore(storeID, force) |
| persistence | `server/api/store.go:141` | 51,52,53,54 | err = rc.RemoveStore(storeID, false) |
| persistence | `server/api/store.go:257` | 51,52,53,54 | if err := rc.DeleteStoreLabel(storeID, labelKey); err != nil { |
| persistence | `server/api/store.go:408` | 51,52,53,54 | err := getCluster(r).RemoveTombStoneRecords() |
| persistence | `server/api/unsafe_operation.go:100` | 102 | if err := rc.GetUnsafeRecoveryController().RemoveFailedStoresWithOptions(stores, timeout, autoDetect, unsaferecovery.RemoveFailedStoresOptions{ |
| persistence | `server/apiv2/handlers/affinity.go:257` | 2,71,72,78 | err = manager.DeleteAffinityGroups(req.IDs, req.Force) |
| persistence | `server/apiv2/handlers/affinity.go:433` | 2,71,72,78 | err = manager.DeleteAffinityGroups([]string{groupID}, force) |
| persistence | `server/apiv2/handlers/keyspace.go:155` | 2,78 | meta, err := manager.LoadKeyspace(name) |
| persistence | `server/apiv2/handlers/keyspace.go:198` | 2,78 | meta, err := manager.LoadKeyspaceByID(uint32(id)) |
| persistence | `server/apiv2/handlers/keyspace.go:275` | 2,78 | scanned, err := manager.LoadRangeKeyspace(scanStart, scanLimit) |
| persistence | `server/apiv2/handlers/maintenance.go:115` | 2,78 | err := svr.GetStorage().LoadAllMaintenanceTasks(func(k, v string) { |
| persistence | `server/apiv2/handlers/maintenance.go:117` | 2,78 | if k == keypath.MaintenanceTaskPath(endpoint.MaintenanceLockName) { |
| persistence | `server/apiv2/handlers/maintenance.go:154` | 2,78 | task, err := svr.GetStorage().LoadMaintenanceTask(taskType) |
| failpoint | `server/apiv2/handlers/ready.go:166` | 2,43,78,94 | failpoint.Inject("loadRegionSlow", func(val failpoint.Value) { |
| failpoint | `server/apiv2/handlers/ready.go:180` | 2,43,78,94 | failpoint.Inject("etcdSerializableReadError", func(val failpoint.Value) { |
| failpoint | `server/apiv2/handlers/ready.go:182` | 2,43,78,94 | failpoint.Return(errors.New("injected serializable read error")) |
| failpoint | `server/apiv2/handlers/ready.go:196` | 2,43,78,94 | failpoint.Inject("etcdLinearizableReadError", func(val failpoint.Value) { |
| failpoint | `server/apiv2/handlers/ready.go:198` | 2,43,78,94 | failpoint.Return(errors.New("injected linearizable read error")) |
| failpoint | `server/apiv2/handlers/ready.go:212` | 2,43,78,94 | failpoint.Inject("etcdDataCorruptionAlarm", func(val failpoint.Value) { |
| failpoint | `server/apiv2/handlers/ready.go:214` | 2,43,78,94 | failpoint.Return(errors.New("injected data corruption alarm")) |
| failpoint | `server/apiv2/handlers/ready.go:229` | 2,43,78,94 | failpoint.Inject("etcdIsLearner", func(val failpoint.Value) { |
| failpoint | `server/apiv2/handlers/ready.go:231` | 2,43,78,94 | failpoint.Return(errors.New("injected learner state")) |
| persistence | `server/apiv2/handlers/tso_keyspace_group.go:264` | 2,78 | kg, err := manager.DeleteKeyspaceGroupByID(id) |
| persistence | `server/apiv2/handlers/tso_keyspace_group.go:673` | 2,78 | kg, err := groupManager.RemoveKeyspacesFromGroup( |
| persistence | `server/apiv2/middlewares/microservice_redirector.go:78` | 102 | pdAffinityPath := "/pd/api/v2/affinity-groups" |
| persistence | `server/apiv2/router.go:36` | 2,78,101 | const apiV2Prefix = "/pd/api/v2/" |
| persistence | `server/cluster/cluster.go:325` | 1,6,12,13,14,15,16,20,27,34,77 | data, err := c.storage.Load(keypath.ClusterBootstrapTimePath()) |
| failpoint | `server/cluster/cluster.go:344` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("syncRegionChannelFull", func() { |
| failpoint | `server/cluster/cluster.go:397` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("raftClusterReturn", func(val failpoint.Value) { |
| failpoint | `server/cluster/cluster.go:403` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Return(err) |
| persistence | `server/cluster/cluster.go:406` | 1,6,12,13,14,15,16,20,27,34,77 | cluster, err := c.LoadClusterInfo() |
| worker | `server/cluster/cluster.go:483` | 1,6,12,13,14,15,16,20,27,34,77 | go c.runServiceCheckJob() |
| worker | `server/cluster/cluster.go:484` | 1,6,12,13,14,15,16,20,27,34,77 | go c.runMetricsCollectionJob() |
| worker | `server/cluster/cluster.go:485` | 1,6,12,13,14,15,16,20,27,34,77 | go c.runNodeStateCheckJob() |
| worker | `server/cluster/cluster.go:486` | 1,6,12,13,14,15,16,20,27,34,77 | go c.syncRegions() |
| worker | `server/cluster/cluster.go:487` | 1,6,12,13,14,15,16,20,27,34,77 | go c.runReplicationMode() |
| worker | `server/cluster/cluster.go:488` | 1,6,12,13,14,15,16,20,27,34,77 | go c.runMinResolvedTSJob() |
| worker | `server/cluster/cluster.go:489` | 1,6,12,13,14,15,16,20,27,34,77 | go c.runStoreConfigSync() |
| worker | `server/cluster/cluster.go:490` | 1,6,12,13,14,15,16,20,27,34,77 | go c.runUpdateStoreStats() |
| worker | `server/cluster/cluster.go:491` | 1,6,12,13,14,15,16,20,27,34,77 | go c.startGCTuner() |
| worker | `server/cluster/cluster.go:492` | 1,6,12,13,14,15,16,20,27,34,77 | go c.startProgressGC() |
| worker | `server/cluster/cluster.go:493` | 1,6,12,13,14,15,16,20,27,34,77 | go c.runStorageSizeCollector(s.GetMeteringWriter(), c.regionLabeler, s.GetKeyspaceManager()) |
| failpoint | `server/cluster/cluster.go:579` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("highFrequencyClusterJobs", func() { |
| failpoint | `server/cluster/cluster.go:633` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("updateAfterResetTSO", func() { |
| failpoint | `server/cluster/cluster.go:723` | 1,6,12,13,14,15,16,20,27,34,77 | // TODO: After we fix the atomic problem of config, we can remove this failpoint. |
| failpoint | `server/cluster/cluster.go:724` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("skipStoreConfigSync", func() { |
| failpoint | `server/cluster/cluster.go:725` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Return() |
| failpoint | `server/cluster/cluster.go:833` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("mockFetchStoreConfigFromTiKV", func(val failpoint.Value) { |
| failpoint | `server/cluster/cluster.go:838` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Return(cfg, nil) |
| persistence | `server/cluster/cluster.go:875` | 1,6,12,13,14,15,16,20,27,34,77 | ok, err := c.storage.LoadMeta(c.meta) |
| persistence | `server/cluster/cluster.go:885` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.LoadStores(c.PutStore); err != nil { |
| failpoint | `server/cluster/cluster.go:912` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("highFrequencyClusterJobs", func() { |
| failpoint | `server/cluster/cluster.go:935` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("highFrequencyClusterJobs", func() { |
| failpoint | `server/cluster/cluster.go:946` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.InjectCall("blockCheckStores") |
| persistence | `server/cluster/cluster.go:1233` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.SaveStoreMeta(newStore.GetMeta()); err != nil { |
| failpoint | `server/cluster/cluster.go:1393` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("concurrentRegionHeartbeat", func() { |
| failpoint | `server/cluster/cluster.go:1399` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("decEpoch", func() { |
| worker | `server/cluster/cluster.go:1459` | 1,6,12,13,14,15,16,20,27,34,77 | // after restart. Here we only log the error then go on updating cache. |
| persistence | `server/cluster/cluster.go:1461` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.DeleteRegion(item.GetMeta()); err != nil { |
| persistence | `server/cluster/cluster.go:1468` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.SaveRegion(region.GetMeta()); err != nil { |
| persistence | `server/cluster/cluster.go:1495` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.SaveMeta(meta); err != nil { |
| failpoint | `server/cluster/cluster.go:1590` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.InjectCall("putMetaStoreAfterWasKnownRead") |
| failpoint | `server/cluster/cluster.go:1815` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.InjectCall("buryStoreBeforeStateLock") |
| failpoint | `server/cluster/cluster.go:1818` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.InjectCall("buryStoreAfterStateLock") |
| persistence | `server/cluster/cluster.go:1856` | 1,6,12,13,14,15,16,20,27,34,77 | c.prevStoreLimit.Delete(storeID) |
| persistence | `server/cluster/cluster.go:1857` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.RemoveStoreLimit(storeID); err != nil { |
| persistence | `server/cluster/cluster.go:1862` | 1,6,12,13,14,15,16,20,27,34,77 | c.ruleManager.RemoveStoreCache(storeID) |
| persistence | `server/cluster/cluster.go:1865` | 1,6,12,13,14,15,16,20,27,34,77 | filter.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/cluster.go:1866` | 1,6,12,13,14,15,16,20,27,34,77 | hbstream.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/cluster.go:1867` | 1,6,12,13,14,15,16,20,27,34,77 | schedule.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/cluster.go:1868` | 1,6,12,13,14,15,16,20,27,34,77 | storeTriggerNetworkSlowEvict.DeleteLabelValues(storeIDStr) |
| persistence | `server/cluster/cluster.go:1872` | 1,6,12,13,14,15,16,20,27,34,77 | if fn := c.onStoreBuried.Load(); fn != nil { |
| persistence | `server/cluster/cluster.go:1933` | 1,6,12,13,14,15,16,20,27,34,77 | if value, ok := c.prevStoreLimit.Load(storeID); ok { |
| persistence | `server/cluster/cluster.go:1997` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.SaveStoreWeight(storeID, leaderWeight, regionWeight); err != nil { |
| persistence | `server/cluster/cluster.go:2010` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.SaveStoreMeta(store.GetMeta()); err != nil { |
| failpoint | `server/cluster/cluster.go:2131` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("doNotBuryStore", func(_ failpoint.Value) { |
| failpoint | `server/cluster/cluster.go:2270` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.InjectCall("removeTombStoneRecordsBeforeStateLock") |
| failpoint | `server/cluster/cluster.go:2273` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.InjectCall("removeTombStoneRecordsAfterStateLock") |
| persistence | `server/cluster/cluster.go:2299` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.RemoveStoreLimit(store.GetID()); err != nil { |
| persistence | `server/cluster/cluster.go:2303` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.DeleteStoreMeta(store.GetMeta()); err != nil { |
| failpoint | `server/cluster/cluster.go:2314` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.InjectCall("deleteStoreBeforeRemoveFromMap") |
| persistence | `server/cluster/cluster.go:2315` | 1,6,12,13,14,15,16,20,27,34,77 | c.DeleteStore(store) |
| persistence | `server/cluster/cluster.go:2316` | 1,6,12,13,14,15,16,20,27,34,77 | c.ruleManager.RemoveStoreCache(store.GetID()) |
| persistence | `server/cluster/cluster.go:2318` | 1,6,12,13,14,15,16,20,27,34,77 | statistics.DeleteClusterStatusMetrics(store) |
| persistence | `server/cluster/cluster.go:2320` | 1,6,12,13,14,15,16,20,27,34,77 | filter.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/cluster.go:2321` | 1,6,12,13,14,15,16,20,27,34,77 | hbstream.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/cluster.go:2322` | 1,6,12,13,14,15,16,20,27,34,77 | schedule.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/cluster.go:2323` | 1,6,12,13,14,15,16,20,27,34,77 | schedulers.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/cluster.go:2324` | 1,6,12,13,14,15,16,20,27,34,77 | scatter.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/cluster.go:2325` | 1,6,12,13,14,15,16,20,27,34,77 | storeTriggerNetworkSlowEvict.DeleteLabelValues(storeIDStr) |
| persistence | `server/cluster/cluster.go:2326` | 1,6,12,13,14,15,16,20,27,34,77 | if fn := c.onStoreBuried.Load(); fn != nil { |
| failpoint | `server/cluster/cluster.go:2377` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.Inject("versionChangeConcurrency", func() { |
| persistence | `server/cluster/cluster.go:2411` | 1,6,12,13,14,15,16,20,27,34,77 | if meta.GetId() != keypath.ClusterID() { |
| persistence | `server/cluster/cluster.go:2412` | 1,6,12,13,14,15,16,20,27,34,77 | return errors.Errorf("invalid cluster %v, mismatch cluster id %d", meta, keypath.ClusterID()) |
| persistence | `server/cluster/cluster.go:2514` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.SaveRegion(region.GetMeta()); err != nil { |
| persistence | `server/cluster/cluster.go:2612` | 1,6,12,13,14,15,16,20,27,34,77 | statistics.StoreLimitGauge.DeleteLabelValues(id, "add-peer") |
| persistence | `server/cluster/cluster.go:2613` | 1,6,12,13,14,15,16,20,27,34,77 | statistics.StoreLimitGauge.DeleteLabelValues(id, "remove-peer") |
| persistence | `server/cluster/cluster.go:2614` | 1,6,12,13,14,15,16,20,27,34,77 | statistics.StoreLimitGauge.DeleteLabelValues(id, "transfer-leader-in") |
| persistence | `server/cluster/cluster.go:2657` | 1,6,12,13,14,15,16,20,27,34,77 | oldMinResolvedTS, _ := c.minResolvedTS.Load().(uint64) |
| persistence | `server/cluster/cluster.go:2685` | 1,6,12,13,14,15,16,20,27,34,77 | if err := c.storage.SaveMinResolvedTS(current); err != nil { |
| persistence | `server/cluster/cluster.go:2700` | 1,6,12,13,14,15,16,20,27,34,77 | minResolvedTS, err := c.GetStorage().LoadMinResolvedTS() |
| persistence | `server/cluster/cluster.go:2713` | 1,6,12,13,14,15,16,20,27,34,77 | return c.minResolvedTS.Load().(uint64) |
| persistence | `server/cluster/cluster.go:2748` | 1,6,12,13,14,15,16,20,27,34,77 | return c.externalTS.Load().(uint64) |
| persistence | `server/cluster/cluster.go:2754` | 1,6,12,13,14,15,16,20,27,34,77 | return c.storage.SaveExternalTS(timestamp) |
| persistence | `server/cluster/cluster.go:2759` | 1,6,12,13,14,15,16,20,27,34,77 | externalTS, err := c.GetStorage().LoadExternalTS() |
| failpoint | `server/cluster/cluster.go:2769` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.InjectCall("setStoreLimitBeforeStateLock") |
| persistence | `server/cluster/cluster.go:2891` | 1,6,12,13,14,15,16,20,27,34,77 | var healthURL = "/pd/api/v1/ping" |
| persistence | `server/cluster/cluster.go:2963` | 1,6,12,13,14,15,16,20,27,34,77 | _, exist := c.independentServices.Load(name) |
| persistence | `server/cluster/cluster.go:2974` | 1,6,12,13,14,15,16,20,27,34,77 | c.independentServices.Delete(name) |
| failpoint | `server/cluster/cluster.go:3010` | 1,6,12,13,14,15,16,20,27,34,77 | failpoint.InjectCall("adjustNetworkSlowStoreBeforeWrite") |
| persistence | `server/cluster/cluster.go:3014` | 1,6,12,13,14,15,16,20,27,34,77 | storeTriggerNetworkSlowEvict.DeleteLabelValues(strconv.FormatUint(storeID, 10)) |
| worker | `server/cluster/cluster.go:3021` | 1,6,12,13,14,15,16,20,27,34,77 | // If the node remains normal, it takes 10 minutes to go from 100 to 1 |
| persistence | `server/cluster/cluster.go:3023` | 1,6,12,13,14,15,16,20,27,34,77 | storeTriggerNetworkSlowEvict.DeleteLabelValues(strconv.FormatUint(storeID, 10)) |
| worker | `server/cluster/scheduling_controller.go:102` | 1,27,34,77 | go sc.runCoordinator() |
| worker | `server/cluster/scheduling_controller.go:103` | 1,27,34,77 | go sc.runStatsBackgroundJobs() |
| worker | `server/cluster/scheduling_controller.go:104` | 1,27,34,77 | go sc.runSchedulingMetricsCollectionJob() |
| failpoint | `server/cluster/scheduling_controller.go:157` | 1,27,34,77 | failpoint.Inject("highFrequencyClusterJobs", func() { |
| persistence | `server/cluster/scheduling_controller.go:205` | 1,27,34,77 | statistics.DeleteClusterStatusMetrics(s) |
| persistence | `server/cluster/scheduling_controller.go:234` | 1,27,34,77 | statistics.StoreLimitGauge.DeleteLabelValues(id, "add-peer") |
| persistence | `server/cluster/scheduling_controller.go:235` | 1,27,34,77 | statistics.StoreLimitGauge.DeleteLabelValues(id, "remove-peer") |
| persistence | `server/cluster/scheduling_controller.go:236` | 1,27,34,77 | statistics.StoreLimitGauge.DeleteLabelValues(id, "transfer-leader-in") |
| persistence | `server/cluster/scheduling_controller.go:253` | 1,27,34,77 | sc.hotStat.RemoveRollingStoreStats(storeID) |
| persistence | `server/cluster/scheduling_controller.go:254` | 1,27,34,77 | sc.slowStat.RemoveSlowStoreStatus(storeID) |
| persistence | `server/cluster/scheduling_controller.go:256` | 1,27,34,77 | schedulers.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/scheduling_controller.go:257` | 1,27,34,77 | scatter.DeleteStoreMetrics(storeIDStr) |
| persistence | `server/cluster/scheduling_controller.go:431` | 1,27,34,77 | return sc.coordinator.GetSchedulersController().RemoveSchedulerHandler(name) |
| persistence | `server/cluster/scheduling_controller.go:445` | 1,27,34,77 | return sc.coordinator.GetSchedulersController().RemoveScheduler(name) |
| persistence | `server/cluster/scheduling_controller.go:480` | 1,27,34,77 | sc.coordinator.GetCheckerController().RemovePendingProcessedRegion(id) |
| persistence | `server/config/persist_options.go:85` | 3,28,47,83,84,85,90 | return o.schedule.Load().(*sc.ScheduleConfig) |
| persistence | `server/config/persist_options.go:139` | 3,28,47,83,84,85,90 | return o.replication.Load().(*sc.ReplicationConfig) |
| persistence | `server/config/persist_options.go:149` | 3,28,47,83,84,85,90 | return o.pdServerConfig.Load().(*PDServerConfig) |
| persistence | `server/config/persist_options.go:159` | 3,28,47,83,84,85,90 | return o.replicationMode.Load().(*ReplicationModeConfig) |
| persistence | `server/config/persist_options.go:169` | 3,28,47,83,84,85,90 | return o.labelProperty.Load().(LabelPropertyConfig) |
| persistence | `server/config/persist_options.go:179` | 3,28,47,83,84,85,90 | return o.keyspace.Load().(*KeyspaceConfig) |
| persistence | `server/config/persist_options.go:194` | 3,28,47,83,84,85,90 | return o.microservice.Load().(*MicroserviceConfig) |
| persistence | `server/config/persist_options.go:204` | 3,28,47,83,84,85,90 | return o.storeConfig.Load().(*sc.StoreConfig) |
| persistence | `server/config/persist_options.go:214` | 3,28,47,83,84,85,90 | return (*semver.Version)(atomic.LoadPointer(&o.clusterVersion)) |
| failpoint | `server/config/persist_options.go:880` | 3,28,47,83,84,85,90 | failpoint.Inject("persistFail", func() { |
| failpoint | `server/config/persist_options.go:881` | 3,28,47,83,84,85,90 | failpoint.Return(errors.New("fail to persist")) |
| persistence | `server/config/persist_options.go:883` | 3,28,47,83,84,85,90 | return storage.SaveConfig(cfg) |
| persistence | `server/config/persist_options.go:894` | 3,28,47,83,84,85,90 | isExist, err := storage.LoadConfig(cfg) |
| persistence | `server/config/persist_options.go:932` | 3,28,47,83,84,85,90 | pc := o.labelProperty.Load().(LabelPropertyConfig) |
| persistence | `server/config/persist_options.go:960` | 3,28,47,83,84,85,90 | _, err := client.Delete(parCtx, sc.TTLConfigPrefix+"/"+key) |
| persistence | `server/config/service_middleware_persist_options.go:45` | 3,28,47,83,84,85,90 | return o.audit.Load().(*AuditConfig) |
| persistence | `server/config/service_middleware_persist_options.go:60` | 3,28,47,83,84,85,90 | return o.rateLimit.Load().(*RateLimitConfig) |
| persistence | `server/config/service_middleware_persist_options.go:75` | 3,28,47,83,84,85,90 | return o.grpcRateLimit.Load().(*GRPCRateLimitConfig) |
| persistence | `server/config/service_middleware_persist_options.go:95` | 3,28,47,83,84,85,90 | err := storage.SaveServiceMiddlewareConfig(cfg) |
| failpoint | `server/config/service_middleware_persist_options.go:96` | 3,28,47,83,84,85,90 | failpoint.Inject("persistServiceMiddlewareFail", func() { |
| persistence | `server/config/service_middleware_persist_options.go:106` | 3,28,47,83,84,85,90 | isExist, err := storage.LoadServiceMiddlewareConfig(cfg) |
| persistence | `server/grpc_service.go:99` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if atomic.LoadInt32(&s.closed) == 1 { |
| worker | `server/grpc_service.go:103` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go func() { |
| failpoint | `server/grpc_service.go:105` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("tsoProxyFailToSendToClient", func() { |
| failpoint | `server/grpc_service.go:107` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Return() |
| persistence | `server/grpc_service.go:126` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if atomic.LoadInt32(&s.closed) == 1 { |
| failpoint | `server/grpc_service.go:129` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("tsoProxyRecvFromClientTimeout", func(val failpoint.Value) { |
| worker | `server/grpc_service.go:135` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go func() { |
| persistence | `server/grpc_service.go:164` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if atomic.LoadInt32(&s.closed) == 1 { |
| worker | `server/grpc_service.go:168` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go func() { |
| persistence | `server/grpc_service.go:188` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if atomic.LoadInt32(&s.closed) == 1 { |
| failpoint | `server/grpc_service.go:230` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("customTimeout", func() { |
| worker | `server/grpc_service.go:340` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go func(addr string) { |
| persistence | `server/grpc_service.go:411` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | ClusterId: keypath.ClusterID(), |
| worker | `server/grpc_service.go:512` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go tsoutil.WatchTSDeadline(stream.Context(), tsDeadlineCh) |
| worker | `server/grpc_service.go:548` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go func() { |
| persistence | `server/grpc_service.go:614` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if clusterID := keypath.ClusterID(); request.GetHeader().GetClusterId() != clusterID { |
| failpoint | `server/grpc_service.go:724` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("handleAllocIDNonBatch", func() { |
| persistence | `server/grpc_service.go:1024` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | pre := s.schedulingClient.Load() |
| persistence | `server/grpc_service.go:1057` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if atomic.LoadInt32(&b.closed) == 1 { |
| worker | `server/grpc_service.go:1061` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go func() { |
| persistence | `server/grpc_service.go:1080` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if atomic.LoadInt32(&b.closed) == 1 { |
| failpoint | `server/grpc_service.go:1119` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("grpcClientClosed", func() { |
| failpoint | `server/grpc_service.go:1136` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("grpcClientClosed", func() { |
| worker | `server/grpc_service.go:1155` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go forwardReportBucketClientToServer(forwardStream, server, errCh) |
| worker | `server/grpc_service.go:1250` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go forwardRegionBucketsToScheduling(forwardSchedulingStream, server, forwardErrCh) |
| failpoint | `server/grpc_service.go:1324` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("grpcClientClosed", func() { |
| worker | `server/grpc_service.go:1343` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go forwardRegionHeartbeatClientToServer(forwardStream, server, errCh) |
| worker | `server/grpc_service.go:1470` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | go forwardRegionHeartbeatToScheduling(rc, forwardSchedulingStream, server, forwardErrCh) |
| failpoint | `server/grpc_service.go:1515` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("rateLimit", func() { |
| failpoint | `server/grpc_service.go:1516` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Return(nil, errs.ErrGRPCRateLimitExceeded(errs.ErrRateLimitExceeded)) |
| failpoint | `server/grpc_service.go:1537` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("delayProcess", nil) |
| persistence | `server/grpc_service.go:1624` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if clusterID := keypath.ClusterID(); request.GetHeader().GetClusterId() != clusterID { |
| failpoint | `server/grpc_service.go:1653` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("queryRegionMetError", func() { |
| failpoint | `server/grpc_service.go:1654` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Return(errs.ErrNotBootstrapped.FastGenByArgs()) |
| failpoint | `server/grpc_service.go:1659` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | failpoint.Inject("queryRegionFollowerCacheMiss", func() { |
| persistence | `server/grpc_service.go:2195` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if clusterID := keypath.ClusterID(); header.GetClusterId() != clusterID { |
| persistence | `server/grpc_service.go:2428` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | const globalConfigPath = "/global/config/" |
| persistence | `server/grpc_service.go:2516` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | if request.Names != nil \|\| configPath != keypath.ControllerConfigPath() { |
| persistence | `server/grpc_service.go:2551` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | // by Etcd.Watch() as long as the context has not been canceled or timed out. |
| persistence | `server/grpc_service.go:2575` | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | watchChan := s.client.Watch(ctx, configPath, clientv3.WithPrefix(), clientv3.WithRev(revision), clientv3.WithPrevKV()) |
| failpoint | `server/join/join.go:191` | 88,89 | failpoint.Inject("addMemberFailed", func() { |
| failpoint | `server/join/join.go:193` | 88,89 | failpoint.Goto("LabelSkipAddMember") |
| failpoint | `server/join/join.go:207` | 88,89 | failpoint.Label("LabelSkipAddMember") |
| persistence | `server/server.go:105` | 1,7,82,94 | pdAPIPrefix           = "/pd/" |
| worker | `server/server.go:366` | 1,7,82,94 | go etcd.Close() |
| failpoint | `server/server.go:474` | 1,7,82,94 | failpoint.Inject("memberNil", func() { |
| failpoint | `server/server.go:601` | 1,7,82,94 | failpoint.InjectCall("delayStartServer") |
| persistence | `server/server.go:690` | 1,7,82,94 | return atomic.LoadInt64(&s.isRunning) == 0 |
| worker | `server/server.go:695` | 1,7,82,94 | go systimemon.StartMonitor(s.ctx, time.Now, func() { |
| failpoint | `server/server.go:709` | 1,7,82,94 | failpoint.Inject("delayStartServerLoop", func() { |
| worker | `server/server.go:740` | 1,7,82,94 | go s.leaderLoop() |
| worker | `server/server.go:741` | 1,7,82,94 | go s.etcdLeaderLoop() |
| worker | `server/server.go:742` | 1,7,82,94 | go s.serverMetricsLoop() |
| worker | `server/server.go:743` | 1,7,82,94 | go s.encryptionKeyManagerLoop() |
| persistence | `server/server.go:793` | 1,7,82,94 | clusterID := keypath.ClusterID() |
| persistence | `server/server.go:815` | 1,7,82,94 | ops = append(ops, clientv3.OpPut(keypath.ClusterPath(), string(clusterValue))) |
| persistence | `server/server.go:821` | 1,7,82,94 | ops = append(ops, clientv3.OpPut(keypath.ClusterBootstrapTimePath(), string(timeData))) |
| persistence | `server/server.go:829` | 1,7,82,94 | ops = append(ops, clientv3.OpPut(keypath.StorePath(storeMeta.GetId()), string(storeValue))) |
| persistence | `server/server.go:837` | 1,7,82,94 | ops = append(ops, clientv3.OpPut(keypath.RegionPath(req.GetRegion().GetId()), string(regionValue))) |
| persistence | `server/server.go:840` | 1,7,82,94 | bootstrapCmp := clientv3.Compare(clientv3.CreateRevision(keypath.ClusterPath()), "=", 0) |
| persistence | `server/server.go:851` | 1,7,82,94 | err = s.storage.SaveRegion(req.GetRegion()) |
| failpoint | `server/server.go:882` | 1,7,82,94 | failpoint.Inject("raftclusterIsBusy", func() {}) |
| persistence | `server/server.go:1027` | 1,7,82,94 | ok, err := s.storage.LoadRegion(regionID, meta) |
| persistence | `server/server.go:1040` | 1,7,82,94 | s.basicCluster.RemoveRegionIfExist(regionID) |
| persistence | `server/server.go:1052` | 1,7,82,94 | endKey := keypath.RegionPath(math.MaxUint64) |
| persistence | `server/server.go:1060` | 1,7,82,94 | keys, _, err := regionKV.LoadRange(keypath.RegionPath(startID), endKey, endpoint.MaxKVRangeLimit) |
| persistence | `server/server.go:1088` | 1,7,82,94 | if err := txn.Remove(key); err != nil { |
| persistence | `server/server.go:1097` | 1,7,82,94 | if err := s.storage.DeleteRegion(region); err != nil { |
| persistence | `server/server.go:1648` | 1,7,82,94 | s.persistOptions.DeleteLabelProperty(typ, labelKey, labelValue) |
| persistence | `server/server.go:1664` | 1,7,82,94 | s.persistOptions.DeleteLabelProperty(typ, labelKey, labelValue) |
| persistence | `server/server.go:1751` | 1,7,82,94 | Id:           keypath.ClusterID(), |
| persistence | `server/server.go:1863` | 1,7,82,94 | return s.cluster.LoadClusterStatus() |
| failpoint | `server/server.go:1932` | 1,7,82,94 | func memberFailpointEnabled(val failpoint.Value, memberID uint64) bool { |
| worker | `server/server.go:1960` | 1,7,82,94 | // add failpoint to test leader check go to stuck. |
| failpoint | `server/server.go:1961` | 1,7,82,94 | failpoint.Inject("leaderLoopCheckAgain", func(val failpoint.Value) { |
| persistence | `server/server.go:1983` | 1,7,82,94 | leader.Watch(s.serverLoopCtx) |
| failpoint | `server/server.go:1998` | 1,7,82,94 | failpoint.Inject("timeoutWaitPDLeader", func(val failpoint.Value) { |
| persistence | `server/server.go:2093` | 1,7,82,94 | if err := s.persistOptions.LoadTTLFromEtcd(s.ctx, s.client); err != nil { |
| failpoint | `server/server.go:2128` | 1,7,82,94 | failpoint.Inject("rebaseErr", func(val failpoint.Value) { |
| failpoint | `server/server.go:2130` | 1,7,82,94 | failpoint.Return() |
| failpoint | `server/server.go:2149` | 1,7,82,94 | failpoint.Inject("blockReadyToServe", func(val failpoint.Value) { |
| failpoint | `server/server.go:2183` | 1,7,82,94 | failpoint.Inject("exitCampaignLeader", func(val failpoint.Value) { |
| failpoint | `server/server.go:2186` | 1,7,82,94 | failpoint.Return() |
| persistence | `server/server.go:2284` | 1,7,82,94 | url := clientUrls[0] + filepath.Join("/pd/api/v1/admin/persist-file", name) |
| persistence | `server/server.go:2362` | 1,7,82,94 | markPath := keypath.RecoveringMarkPath() |
| persistence | `server/server.go:2374` | 1,7,82,94 | resp, err := s.client.Get(ctx, keypath.RecoveringMarkPath()) |
| persistence | `server/server.go:2384` | 1,7,82,94 | _, err := s.client.Delete(ctx, keypath.RecoveringMarkPath()) |
| persistence | `server/server.go:2392` | 1,7,82,94 | markPath := keypath.PitrRestoreModeMarkPath() |
| persistence | `server/server.go:2404` | 1,7,82,94 | resp, err := s.client.Get(ctx, keypath.PitrRestoreModeMarkPath()) |
| persistence | `server/server.go:2414` | 1,7,82,94 | _, err := s.client.Delete(ctx, keypath.PitrRestoreModeMarkPath()) |
| persistence | `server/server.go:2425` | 1,7,82,94 | if v, ok := s.servicePrimaryMap.Load(serviceName); ok { |
| persistence | `server/server.go:2454` | 1,7,82,94 | s.servicePrimaryMap.Delete(serviceName) |
| persistence | `server/server.go:2459` | 1,7,82,94 | tsoServicePrimaryKey := keypath.ElectionPath(&keypath.MsParam{ |
| persistence | `server/server.go:2469` | 1,7,82,94 | primaryKey := keypath.ElectionPath(&keypath.MsParam{ |
| persistence | `server/server.go:2478` | 1,7,82,94 | primaryKey := keypath.ElectionPath(&keypath.MsParam{ |
| persistence | `server/server.go:2501` | 1,7,82,94 | v, ok := s.servicePrimaryMap.Load(serviceName) |
| persistence | `server/server.go:2506` | 1,7,82,94 | s.servicePrimaryMap.Delete(serviceName) |
| worker | `tests/rust-compat/harness.rs:86` | 4,98,99,100,101 | child: command.spawn()?, |
| persistence | `tests/rust-compat/harness.rs:107` | 4,98,99,100,101 | if let Ok((200, body)) = self.get("/pd/api/v1/health") { |
| persistence | `tests/rust-compat/harness.rs:112` | 4,98,99,100,101 | if let Ok((200, members)) = self.get("/pd/api/v1/members") { |
| persistence | `tests/rust-compat/harness.rs:151` | 4,98,99,100,101 | let (hs, h) = self.get("/pd/api/v1/health")?; |
| persistence | `tests/rust-compat/harness.rs:152` | 4,98,99,100,101 | let (ms, m) = self.get("/pd/api/v1/members")?; |
| persistence | `tests/rust-compat/harness.rs:153` | 4,98,99,100,101 | let (cs, c) = self.get("/pd/api/v1/config")?; |
| persistence | `tests/rust-compat/harness.rs:216` | 4,98,99,100,101 | let (hs, h) = server.get("/pd/api/v1/health")?; |
| persistence | `tests/rust-compat/harness.rs:217` | 4,98,99,100,101 | let (ms, m) = server.get("/pd/api/v1/members")?; |
| persistence | `tests/rust-compat/state_oracle.rs:308` | 100,101 | format!("/pd/{id}/raft/r/").as_bytes(), |
| persistence | `tests/rust-compat/state_oracle.rs:450` | 100,101 | format!("/pd/{id}/raft/r/").as_bytes(), |
| persistence | `tests/server/apiv2/handlers/testutil.go:37` | 99 | v2Prefix                = "/pd/api/v2" |
| persistence | `tests/server/apiv2/handlers/testutil.go:38` | 99 | keyspacesPrefix         = "/pd/api/v2/keyspaces" |
| persistence | `tests/server/apiv2/handlers/testutil.go:39` | 99 | keyspaceGroupsPrefix    = "/pd/api/v2/tso/keyspace-groups" |
| persistence | `tests/server/apiv2/handlers/testutil.go:40` | 99 | metaServiceGroupsPrefix = "/pd/api/v2/meta-service-groups" |

## Go集群worker责任核对

`server/cluster/cluster.go::Start`启动以下worker；共享状态由5/6先接线，所有worker取消/join统一由95验收。

| Start启动调用 | owner | 边界与验收缺口 |
| --- | --- | --- |
| runServiceCheckJob | 7、37、78、95、102 | classic/MCS角色检查；独立MCS不在范围，需记录明确拒绝/兼容缺口 |
| runMetricsCollectionJob | 15、86、95 | 宿主Registry、定时采集与取消 |
| runNodeStateCheckJob | 12、95 | store状态转换与故障注入 |
| syncRegions | 11、95 | region同步重连/快照/join |
| runReplicationMode | 49、78、95 | 主题8 replication领域准入未确认，阻塞完整装配 |
| runMinResolvedTSJob | 16、34、95 | 聚合/持久化/重启 |
| runStoreConfigSync | 14、95 | TiKV配置读取、失败重试 |
| runUpdateStoreStats | 13、95 | 状态共享与周期更新 |
| startGCTuner | 95、102 | GC tuner是Go runtime相关能力，需验收明确不适用边界，不直接照搬 |
| startProgressGC | 54、95 | 进度GC和取消 |
| runStorageSizeCollector | 78、95、102 | metering/keyspace领域依赖未确认；不以空worker代替 |

## 持久化键定义补充

Go keypath常量按原文固定，涵盖绝对和相对路径；GC/keyspace/resource等键只记录主题8领域边界，不能据此要求适配层自行实现。实际key/事务、watch及重启差分在所属任务验收。

| 定义位置 | owner（按用途细分） | 常量原文 |
| --- | --- | --- |
| `pkg/utils/keypath/absolute_key_path.go:29` | 6、20、32–35、48–50、69–71、78、102 | GCWorkerServiceSafePointID = "gc_worker" |
| `pkg/utils/keypath/absolute_key_path.go:31` | 6、20、32–35、48–50、69–71、78、102 | NativeBRServiceSafePointID = "native_br" |
| `pkg/utils/keypath/absolute_key_path.go:37` | 6、20、32–35、48–50、69–71、78、102 | ClusterIDPath = "/pd/cluster_id" // "/pd/cluster_id" |
| `pkg/utils/keypath/absolute_key_path.go:39` | 6、20、32–35、48–50、69–71、78、102 | leaderPathFormat            = "/pd/%d/leader"                             // "/pd/{cluster_id}/leader" |
| `pkg/utils/keypath/absolute_key_path.go:40` | 6、20、32–35、48–50、69–71、78、102 | allocIDPathFormat           = "/pd/%d/alloc_id"                           // "/pd/{cluster_id}/alloc_id" |
| `pkg/utils/keypath/absolute_key_path.go:41` | 6、20、32–35、48–50、69–71、78、102 | keyspaceAllocIDPathFormat   = "/pd/%d/keyspaces/alloc_id"                 // "/pd/{cluster_id}/keyspaces/alloc_id" |
| `pkg/utils/keypath/absolute_key_path.go:42` | 6、20、32–35、48–50、69–71、78、102 | configPathFormat            = "/pd/%d/config"                             // "/pd/{cluster_id}/config" |
| `pkg/utils/keypath/absolute_key_path.go:43` | 6、20、32–35、48–50、69–71、78、102 | schedulerConfigPathFormat   = "/pd/%d/scheduler_config/%s"                // "/pd/{cluster_id}/scheduler_config/{scheduler_name}" |
| `pkg/utils/keypath/absolute_key_path.go:44` | 6、20、32–35、48–50、69–71、78、102 | storeLeaderWeightPathFormat = "/pd/%d/schedule/store_weight/%020d/leader" // "/pd/{cluster_id}/schedule/store_weight/{store_id}/leader" |
| `pkg/utils/keypath/absolute_key_path.go:45` | 6、20、32–35、48–50、69–71、78、102 | storeRegionWeightPathFormat = "/pd/%d/schedule/store_weight/%020d/region" // "/pd/{cluster_id}/schedule/store_weight/{store_id}/region" |
| `pkg/utils/keypath/absolute_key_path.go:47` | 6、20、32–35、48–50、69–71、78、102 | serviceMiddlewarePathFormat   = "/pd/%d/service_middleware"                  // "/pd/{cluster_id}/service_middleware" |
| `pkg/utils/keypath/absolute_key_path.go:48` | 6、20、32–35、48–50、69–71、78、102 | replicationModePathFormat     = "/pd/%d/replication_mode/%s"                 // "/pd/{cluster_id}/replication_mode/{mode}" |
| `pkg/utils/keypath/absolute_key_path.go:49` | 6、20、32–35、48–50、69–71、78、102 | recoveringMarkPathFormat      = "/pd/%d/cluster/markers/snapshot-recovering" // "/pd/{cluster_id}/cluster/markers/snapshot-recovering" |
| `pkg/utils/keypath/absolute_key_path.go:50` | 6、20、32–35、48–50、69–71、78、102 | pitrRestoreModeMarkPathFormat = "/pd/%d/cluster/markers/pitr-restore-mode"   // "/pd/{cluster_id}/cluster/markers/pitr-restore-mode" |
| `pkg/utils/keypath/absolute_key_path.go:52` | 6、20、32–35、48–50、69–71、78、102 | memberBinaryDeployPathFormat   = "/pd/%d/member/%d/deploy_path"     // "/pd/{cluster_id}/member/{member_id}/deploy_path" |
| `pkg/utils/keypath/absolute_key_path.go:53` | 6、20、32–35、48–50、69–71、78、102 | memberGitHashPath              = "/pd/%d/member/%d/git_hash"        // "/pd/{cluster_id}/member/{member_id}/git_hash" |
| `pkg/utils/keypath/absolute_key_path.go:54` | 6、20、32–35、48–50、69–71、78、102 | memberBinaryVersionPathFormat  = "/pd/%d/member/%d/binary_version"  // "/pd/{cluster_id}/member/{member_id}/binary_version" |
| `pkg/utils/keypath/absolute_key_path.go:55` | 6、20、32–35、48–50、69–71、78、102 | memberLeaderPriorityPathFormat = "/pd/%d/member/%d/leader_priority" // "/pd/{cluster_id}/member/{member_id}/leader_priority" |
| `pkg/utils/keypath/absolute_key_path.go:57` | 6、20、32–35、48–50、69–71、78、102 | rulePathFormat = "/pd/%d/rules/%s" // "/pd/{cluster_id}/rules/{rule_id}" |
| `pkg/utils/keypath/absolute_key_path.go:59` | 6、20、32–35、48–50、69–71、78、102 | ruleCommonPrefixFormat  = "/pd/%d/rule"            // "/pd/{cluster_id}/rule" |
| `pkg/utils/keypath/absolute_key_path.go:60` | 6、20、32–35、48–50、69–71、78、102 | ruleGroupPathFormat     = "/pd/%d/rule_group/%s"   // "/pd/{cluster_id}/rule_group/{group_id}" |
| `pkg/utils/keypath/absolute_key_path.go:61` | 6、20、32–35、48–50、69–71、78、102 | regionLabelPathFormat   = "/pd/%d/region_label/%s" // "/pd/{cluster_id}/region_label/{label_id}" |
| `pkg/utils/keypath/absolute_key_path.go:62` | 6、20、32–35、48–50、69–71、78、102 | regionLabelPrefixFormat = "/pd/%d/region_label/"   // "/pd/{cluster_id}/region_label/" |
| `pkg/utils/keypath/absolute_key_path.go:65` | 6、20、32–35、48–50、69–71、78、102 | maintenanceTaskPathFormat = "/pd/%d/maintenance/%s" // "/pd/{cluster_id}/maintenance/{task_type}" |
| `pkg/utils/keypath/absolute_key_path.go:70` | 6、20、32–35、48–50、69–71、78、102 | gcStateRevisionPathFormat            = "/pd/%d/gc/gc_state_revision"         // "/pd/{cluster_id}/gc/gc_state_revision" |
| `pkg/utils/keypath/absolute_key_path.go:71` | 6、20、32–35、48–50、69–71、78、102 | unifiedGCSafePointPathFormat         = "/pd/%d/gc/safe_point"                // "/pd/{cluster_id}/gc/safe_point" |
| `pkg/utils/keypath/absolute_key_path.go:72` | 6、20、32–35、48–50、69–71、78、102 | keyspaceLevelGCSafePointPrefixFormat = "/pd/%d/keyspaces/gc_safe_point/"     // "/pd/{cluster_id}/keyspaces/gc_safe_point/" |
| `pkg/utils/keypath/absolute_key_path.go:73` | 6、20、32–35、48–50、69–71、78、102 | keyspaceLevelGCSafePointPathFormat   = "/pd/%d/keyspaces/gc_safe_point/%08d" // "/pd/{cluster_id}/keyspaces/gc_safe_point/{keyspace_id}" |
| `pkg/utils/keypath/absolute_key_path.go:75` | 6、20、32–35、48–50、69–71、78、102 | unifiedTxnSafePointPath = "/tidb/store/gcworker/saved_safe_point" |
| `pkg/utils/keypath/absolute_key_path.go:77` | 6、20、32–35、48–50、69–71、78、102 | keyspaceLevelTxnSafePointPath     = "/keyspaces/tidb/%d/tidb/store/gcworker/saved_safe_point" // "/keyspaces/tidb/{keyspace_id}/tidb/store/gcworker/saved_safe_point" |
| `pkg/utils/keypath/absolute_key_path.go:78` | 6、20、32–35、48–50、69–71、78、102 | unifiedGCBarrierPathFormat        = "/pd/%d/gc/safe_point/service/%s"                         // "/pd/{cluster_id}/gc/safe_point/service/{barrier_id}" |
| `pkg/utils/keypath/absolute_key_path.go:79` | 6、20、32–35、48–50、69–71、78、102 | keyspaceLevelGCBarrierPathFormat  = "/pd/%d/keyspaces/service_safe_point/%08d/%s"             // "/pd/{cluster_id}/keyspaces/service_safe_point/{keyspace_id}/{barrier_id}" |
| `pkg/utils/keypath/absolute_key_path.go:80` | 6、20、32–35、48–50、69–71、78、102 | globalGCBarrierPathFormat         = "/pd/%d/gc/global_gc_barriers/%s"                         // "/pd/{cluster_id}/gc/global_gc_barriers/{barrier_id}" |
| `pkg/utils/keypath/absolute_key_path.go:81` | 6、20、32–35、48–50、69–71、78、102 | globalGCBarrierPathPrefix         = "/pd/%d/gc/global_gc_barriers/" |
| `pkg/utils/keypath/absolute_key_path.go:82` | 6、20、32–35、48–50、69–71、78、102 | unifiedTiDBMinStartTSPrefix       = "/tidb/server/minstartts/" |
| `pkg/utils/keypath/absolute_key_path.go:83` | 6、20、32–35、48–50、69–71、78、102 | keyspaceLevelTiDBMinStartTSPrefix = "/keyspaces/tidb/%d/tidb/server/minstartts/" // "/keyspaces/tidb/{keyspace_id}/tidb/server/minstartts" |
| `pkg/utils/keypath/absolute_key_path.go:88` | 6、20、32–35、48–50、69–71、78、102 | clusterPathFormat              = "/pd/%d/raft"                            // "/pd/{cluster_id}/raft" |
| `pkg/utils/keypath/absolute_key_path.go:89` | 6、20、32–35、48–50、69–71、78、102 | clusterBootstrapTimePathFormat = "/pd/%d/raft/status/raft_bootstrap_time" // "/pd/{cluster_id}/raft/status/raft_bootstrap_time" |
| `pkg/utils/keypath/absolute_key_path.go:90` | 6、20、32–35、48–50、69–71、78、102 | storePathPrefixFormat          = "/pd/%d/raft/s/"                         // "/pd/{cluster_id}/raft/s/" |
| `pkg/utils/keypath/absolute_key_path.go:91` | 6、20、32–35、48–50、69–71、78、102 | storePathFormat                = "/pd/%d/raft/s/%020d"                    // "/pd/{cluster_id}/raft/s/{store_id}" |
| `pkg/utils/keypath/absolute_key_path.go:92` | 6、20、32–35、48–50、69–71、78、102 | minResolvedTSPathFormat        = "/pd/%d/raft/min_resolved_ts"            // "/pd/{cluster_id}/raft/min_resolved_ts" |
| `pkg/utils/keypath/absolute_key_path.go:93` | 6、20、32–35、48–50、69–71、78、102 | externalTimestampPathFormat    = "/pd/%d/raft/external_timestamp"         // "/pd/{cluster_id}/raft/external_timestamp" |
| `pkg/utils/keypath/absolute_key_path.go:95` | 6、20、32–35、48–50、69–71、78、102 | keyspaceMetaPrefixFormat    = "/pd/%d/keyspaces/meta/"                     // "/pd/{cluster_id}/keyspaces/meta/" |
| `pkg/utils/keypath/absolute_key_path.go:96` | 6、20、32–35、48–50、69–71、78、102 | keyspaceMetaPathFormat      = "/pd/%d/keyspaces/meta/%08d"                 // "/pd/{cluster_id}/keyspaces/meta/{keyspace_id}" |
| `pkg/utils/keypath/absolute_key_path.go:97` | 6、20、32–35、48–50、69–71、78、102 | keyspaceIDPathFormat        = "/pd/%d/keyspaces/id/%s"                     // "/pd/{cluster_id}/keyspaces/id/{keyspace_name}" |
| `pkg/utils/keypath/absolute_key_path.go:98` | 6、20、32–35、48–50、69–71、78、102 | keyspaceGroupIDPrefixFormat = "/pd/%d/tso/keyspace_groups/membership/"     // "/pd/{cluster_id}/tso/keyspace_groups/membership/" |
| `pkg/utils/keypath/absolute_key_path.go:99` | 6、20、32–35、48–50、69–71、78、102 | keyspaceGroupIDPathFormat   = "/pd/%d/tso/keyspace_groups/membership/%05d" // "/pd/{cluster_id}/tso/keyspace_groups/membership/{group_id}" |
| `pkg/utils/keypath/absolute_key_path.go:102` | 6、20、32–35、48–50、69–71、78、102 | servicePathFormat  = "/ms/%d/%s/registry/"   // "/ms/{cluster_id}/{service_name}/registry/" |
| `pkg/utils/keypath/absolute_key_path.go:103` | 6、20、32–35、48–50、69–71、78、102 | registryPathFormat = "/ms/%d/%s/registry/%s" // "/ms/{cluster_id}/{service_name}/registry/{service_addr}" |
| `pkg/utils/keypath/absolute_key_path.go:105` | 6、20、32–35、48–50、69–71、78、102 | msPrimaryPathFormat           = "/ms/%d/%s/primary"                                // "/ms/{cluster_id}/{service_name}/primary" |
| `pkg/utils/keypath/absolute_key_path.go:106` | 6、20、32–35、48–50、69–71、78、102 | msTsoDefaultPrimaryPathFormat = "/ms/%d/tso/00000/primary"                         // "/ms/{cluster_id}/tso/00000/primary" |
| `pkg/utils/keypath/absolute_key_path.go:107` | 6、20、32–35、48–50、69–71、78、102 | msTsoKespacePrimaryPathFormat = "/ms/%d/tso/keyspace_groups/election/%05d/primary" // "/ms/{cluster_id}/tso/keyspace_groups/election/{group_id}/primary" |
| `pkg/utils/keypath/absolute_key_path.go:115` | 6、20、32–35、48–50、69–71、78、102 | msExpectedPrimaryPathFormat            = "/ms/%d/%s/primary/expected_primary"                                // "/ms/{cluster_id}/{service_name}/primary/expected_primary" |
| `pkg/utils/keypath/absolute_key_path.go:116` | 6、20、32–35、48–50、69–71、78、102 | msTsoDefaultExpectedPrimaryPathFormat  = "/ms/%d/tso/00000/primary/expected_primary"                         // "/ms/{cluster_id}/tso/00000/primary" |
| `pkg/utils/keypath/absolute_key_path.go:117` | 6、20、32–35、48–50、69–71、78、102 | msTsoKeyspaceExpectedPrimaryPathFormat = "/ms/%d/tso/keyspace_groups/election/%05d/primary/expected_primary" // "/ms/{cluster_id}/tso/keyspace_groups/election/{group_id}/primary" |
| `pkg/utils/keypath/absolute_key_path.go:120` | 6、20、32–35、48–50、69–71、78、102 | keyspaceResourceGroupSettingsPathPrefixFormat = "resource_group/keyspace/settings/"      // "resource_group/keyspace/settings/" |
| `pkg/utils/keypath/absolute_key_path.go:121` | 6、20、32–35、48–50、69–71、78、102 | keyspaceResourceGroupSettingsPathFormat       = "resource_group/keyspace/settings/%d/%s" // "resource_group/keyspace/settings/{keyspace_id}/{group_name}" |
| `pkg/utils/keypath/absolute_key_path.go:122` | 6、20、32–35、48–50、69–71、78、102 | keyspaceResourceGroupStatesPathPrefixFormat   = "resource_group/keyspace/states/"        // "resource_group/keyspace/states/" |
| `pkg/utils/keypath/absolute_key_path.go:123` | 6、20、32–35、48–50、69–71、78、102 | keyspaceResourceGroupStatesPathFormat         = "resource_group/keyspace/states/%d/%s"   // "resource_group/keyspace/states/{keyspace_id}/{group_name}" |
| `pkg/utils/keypath/absolute_key_path.go:125` | 6、20、32–35、48–50、69–71、78、102 | keyspaceServiceLimitsPathPrefixFormat = "resource_group/keyspace/service_limits/"   // "resource_group/keyspace/service_limits/" |
| `pkg/utils/keypath/absolute_key_path.go:126` | 6、20、32–35、48–50、69–71、78、102 | keyspaceServiceLimitsPathFormat       = "resource_group/keyspace/service_limits/%d" // "resource_group/keyspace/service_limits/{keyspace_id}" |
| `pkg/utils/keypath/absolute_key_path.go:129` | 6、20、32–35、48–50、69–71、78、102 | resourceGroupSettingsPathFormat = "resource_group/settings/%s" // "resource_group/settings/{group_name}" |
| `pkg/utils/keypath/absolute_key_path.go:130` | 6、20、32–35、48–50、69–71、78、102 | resourceGroupStatesPathFormat   = "resource_group/states/%s"   // "resource_group/states/{group_name}" |
| `pkg/utils/keypath/absolute_key_path.go:131` | 6、20、32–35、48–50、69–71、78、102 | controllerConfigPath            = "resource_group/controller"  // "resource_group/controller" |
| `pkg/utils/keypath/absolute_key_path.go:133` | 6、20、32–35、48–50、69–71、78、102 | timestampPathFormat   = "/pd/%d/timestamp"              // "/pd/{cluster_id}/timestamp" |
| `pkg/utils/keypath/absolute_key_path.go:134` | 6、20、32–35、48–50、69–71、78、102 | msTimestampPathFormat = "/ms/%d/tso/%05d/gta/timestamp" // "/ms/{cluster_id}/tso/{group_id}/gta/timestamp" |
| `pkg/utils/keypath/absolute_key_path.go:136` | 6、20、32–35、48–50、69–71、78、102 | affinityGroupPathFormat = "/pd/%d/affinity_groups/%s" // "/pd/{cluster_id}/affinity_groups/{group_id}" |
| `pkg/utils/keypath/absolute_key_path.go:139` | 6、20、32–35、48–50、69–71、78、102 | metaServiceGroupStatusFormat = "/pd/%d/meta_service_groups/%s/status" // "/pd/{cluster_id}/meta_service_groups/{group_id}/status" |
| `pkg/utils/keypath/absolute_key_path.go:282` | 6、20、32–35、48–50、69–71、78、102 | regionPathPrefix = "raft/r" |

## 完整来源SHA-256 manifest

每行均有owner。此表哈希冻结源码/测试/依赖事实，不为表内每个文件宣称已迁移或已运行。

| 来源文件 | owner | SHA-256 |
| --- | --- | --- |
| [`.github/copilot-instructions.md`](../../../.github/copilot-instructions.md) | 102 | `21f26123b1c90b854735cb46f3ecc41d02c5be7cfa6c89f8840d96624a78e52b` |
| [`.plans/2026-10-22-pd-rust-渐进迁移主题/10-实现生产元数据后端与发布切换.md`](../../../.plans/2026-10-22-pd-rust-渐进迁移主题/10-实现生产元数据后端与发布切换.md) | 102 | `ea95c6bc10428b4303ce1eb5a34a4a23aed22946546b008dc576d193d974cb76` |
| [`.plans/2026-10-22-pd-rust-渐进迁移主题/7-补齐PD服务端与API.md`](../../../.plans/2026-10-22-pd-rust-渐进迁移主题/7-补齐PD服务端与API.md) | 102 | `f01ee4f06c5608e439e774de63fc11828774f049edfbceb224674105e7c27f75` |
| [`.plans/2026-10-22-pd-rust-渐进迁移主题/8-迁移控制面服务与MCS.md`](../../../.plans/2026-10-22-pd-rust-渐进迁移主题/8-迁移控制面服务与MCS.md) | 78 | `ea04b21c53239ce1962f6e04952bc061420f7a7b1b281472facb8e62ec9e5128` |
| [`.plans/2026-10-22-pd-rust-渐进迁移主题/9-迁移运维客户端与工具.md`](../../../.plans/2026-10-22-pd-rust-渐进迁移主题/9-迁移运维客户端与工具.md) | 102 | `06be9a4a1831612eae8371af533bbde00e238f7e09b7f6614de258a405f25cf0` |
| [`.plans/2026-10-27-pd-rust-服务端与API/plan.md`](../../../.plans/2026-10-27-pd-rust-服务端与API/plan.md) | 102 | `a59eba65612e814ada50274525cbb41a81df792f2d2966c2e60a2deae5d84615` |
| [`.plans/2026-10-27-pd-rust-服务端与API/prompt.md`](../../../.plans/2026-10-27-pd-rust-服务端与API/prompt.md) | 102 | `97a6d59dd48130fe9d0888a9f8c42e6d1e74ff141ec8b985a71d06718fedb377` |
| [`AGENTS.md`](../../../AGENTS.md) | 102 | `a7deeb5123e3af7d0a9ed2cbba1b679dc9174b25d426f0788430871c09461200` |
| [`Cargo.lock`](../../../Cargo.lock) | 102 | `ce2f7200b5f735aede4a237681b1fcdc51e18d1ff97b90710d2b62b067adf796` |
| [`Cargo.toml`](../../../Cargo.toml) | 102 | `a29ff6c3b151f8bc7d07287233fb0453f3d1012fd4dbba7bd166b39bdd94e991` |
| [`client/Makefile`](../../../client/Makefile) | 99 | `d7a663b4fb8b1645925c4339230039d4878bf5b89058c457fc5ff1591bee8a85` |
| [`client/client.go`](../../../client/client.go) | 99 | `3fd0d6a4f81ae53d45bca14f77e60189929e3b0fe88ba0e47989c0b8c551dd99` |
| [`client/client_test.go`](../../../client/client_test.go) | 99 | `d9b53c4e15ec6f07dad9f61cdd04b0627b72e79931c6608dc40f8e00dacc9520` |
| [`client/clients/gc/client.go`](../../../client/clients/gc/client.go) | 99 | `130083e5fbdc47a744d598924bb15ac0e16aa6959b9e0261a80332a7d187ff35` |
| [`client/clients/gc/client_test.go`](../../../client/clients/gc/client_test.go) | 99 | `629c00fa5ea9f752a6ac86fbd18b65846eb972c2088b2b972866c2fa5ab69688` |
| [`client/clients/metastorage/client.go`](../../../client/clients/metastorage/client.go) | 99 | `4143723d6ca950aa0e86beded2502fff663a74e2986b364da34c47c676099373` |
| [`client/clients/router/client.go`](../../../client/clients/router/client.go) | 99 | `c966b535478fd3dd8505fa232357b3fc7a834eade0adec3f917b5aa2ce24b993` |
| [`client/clients/router/client_test.go`](../../../client/clients/router/client_test.go) | 99 | `224002e9f80c188b8d7c1b5f92ea7fc7704d70445473fcdef491f780f6adb4fc` |
| [`client/clients/router/request.go`](../../../client/clients/router/request.go) | 99 | `2aefd1846f0709dc91f3573442cd94fa2069f37e952f362a53bd81e87e827078` |
| [`client/clients/tso/client.go`](../../../client/clients/tso/client.go) | 99 | `b0dfc87a67b9da975ff59cb9b353390f63b2d1a22a6195cc9a4446d597ff8ce1` |
| [`client/clients/tso/dispatcher.go`](../../../client/clients/tso/dispatcher.go) | 99 | `08bf54eb52fe26142fcd2b5dcbe6487e56acccb1e4e41090c6fe6f858dc19820` |
| [`client/clients/tso/dispatcher_test.go`](../../../client/clients/tso/dispatcher_test.go) | 99 | `0454497214b697961d1ba82fb91afd3678d09f53bd791b4df7b36501bca6c5fc` |
| [`client/clients/tso/request.go`](../../../client/clients/tso/request.go) | 99 | `b3d69292c26b3da4ae46323f4af2ea4e4fbe32d1565c2628968385d7c38d523b` |
| [`client/clients/tso/request_test.go`](../../../client/clients/tso/request_test.go) | 99 | `3ae4662de27a60b517f7ec2389abcdf43113b410e1a0b3eb210419567bb587a9` |
| [`client/clients/tso/stream.go`](../../../client/clients/tso/stream.go) | 99 | `3429bfeae2d3d9386a866708fcc6694c26245dc6e098bfed70356ce83951ca36` |
| [`client/clients/tso/stream_test.go`](../../../client/clients/tso/stream_test.go) | 99 | `56e2d935bb2bfede1dbf34df0ae2575524cb71817083f0659b9fec9aed8f323a` |
| [`client/constants/constants.go`](../../../client/constants/constants.go) | 99 | `53f1d693eaf029eacdc6908a61908bb50b186cdc5108190836289324d862e26f` |
| [`client/errs/errno.go`](../../../client/errs/errno.go) | 99 | `32fc4ed013f6d810b7d19d3de4634a83faff48359b7e3fcb25ff76633e84fbe2` |
| [`client/errs/errs.go`](../../../client/errs/errs.go) | 99 | `e3af949b6f5db4c8751ed914d77c54d9665babe8b56efe288e83d72e3a64352a` |
| [`client/gc_client.go`](../../../client/gc_client.go) | 99 | `c8ba5d99c349e5ddf64739f69c85cc1032c52e8e2f0b753f9d9d79fd5e492967` |
| [`client/gc_client_test.go`](../../../client/gc_client_test.go) | 99 | `c3b1b9969b4fa90f7f74a94d39c5c97b9932e8c71594b045fc4a8ee3496e78dc` |
| [`client/go.mod`](../../../client/go.mod) | 99 | `3826b071f12ad9ef8c9f51b95221b1e84a27e9cb13db9219acc638c77f382f29` |
| [`client/go.sum`](../../../client/go.sum) | 99 | `5cb4741539143fb27c9201364dbed93930da441f64b9575cddf68a335569fcb1` |
| [`client/http/api.go`](../../../client/http/api.go) | 99 | `cd1b350a84f233100d57f57107061e873634a437ab55a73228cc6aadef39eb84` |
| [`client/http/client.go`](../../../client/http/client.go) | 99 | `0384ace37a36ffc3565f78bb0699797c655f26d6f368eab50887e92bf07c69b7` |
| [`client/http/client_test.go`](../../../client/http/client_test.go) | 99 | `fd21c92517a7a5b1399a470e282674018d684f85e46a43b84898da2c1670ba34` |
| [`client/http/codec.go`](../../../client/http/codec.go) | 99 | `d10a4a74dc1919e23f7ac220448e8756a7ce7a88a389311cd268a541b0474771` |
| [`client/http/codec_test.go`](../../../client/http/codec_test.go) | 99 | `8fd85660a606c877c15d794a4c16b584b84629e1da970fabf58fa7e23b1cbb0b` |
| [`client/http/interface.go`](../../../client/http/interface.go) | 99 | `d69729c79133e07d9d161fea2e8011a6c335fb5061d1fdc525fcc735f84e14d5` |
| [`client/http/request_info.go`](../../../client/http/request_info.go) | 99 | `88b8f10db059ce93845c9a8ae7035c76227c02a9f276bcefb86833dce1085365` |
| [`client/http/testutil.go`](../../../client/http/testutil.go) | 99 | `1796bf104b20b333956b009642bd805a9d9452b1c8fe7987b68c3ec312bd7cd1` |
| [`client/http/types.go`](../../../client/http/types.go) | 99 | `9a9139420b2f781cf402005a7b326a7cb92bebaad5d901dad5d3d1a9b23d58ba` |
| [`client/http/types_test.go`](../../../client/http/types_test.go) | 99 | `d7a6658b130992c994e0bebc2e466e3c1c53ce8df3614a4b327d6c33614bd7a7` |
| [`client/inner_client.go`](../../../client/inner_client.go) | 99 | `1e5cb053f34fddaed8f2f06800fb11e51bf7094a3ff878b119c6678a279f146a` |
| [`client/keyspace_client.go`](../../../client/keyspace_client.go) | 99 | `860d64ad148a902b9c85c04fff4c805bdbb7cfd6b8bb2d6447f7c0ee8704129e` |
| [`client/keyspace_client_test.go`](../../../client/keyspace_client_test.go) | 99 | `ff84e63d377d38373972db311560524ffdf0d861b423eb0fac2ce0c0ac7ae544` |
| [`client/meta_storage_client.go`](../../../client/meta_storage_client.go) | 99 | `5adfd9af711d72b27a0c74c25663eb19760399bac52553fc7614f9f11aff3c2a` |
| [`client/metrics/metrics.go`](../../../client/metrics/metrics.go) | 99 | `9d966d40fbe46d4d5d925ed59d2a0291fa2f812f17a1029f48b535385ef8d34b` |
| [`client/opt/option.go`](../../../client/opt/option.go) | 99 | `4799581c1573b9ec7dab1daab437146452e2ee3e1e33d26ece2d5e7727f272c1` |
| [`client/opt/option_test.go`](../../../client/opt/option_test.go) | 99 | `c17d90178b6373fa6e7d2ddc15530ce30276aa032b1fbab75204e5798f9a794e` |
| [`client/pkg/batch/batch_controller.go`](../../../client/pkg/batch/batch_controller.go) | 99 | `a2b42def060b2b01c6bb07059252a1106e8a72600db6f95fcbabe9da44c1d627` |
| [`client/pkg/batch/batch_controller_test.go`](../../../client/pkg/batch/batch_controller_test.go) | 99 | `bdf2cddb4fa5fec513c0dc83263be09a751cf3eb8d18b84f15aad867321dfcd4` |
| [`client/pkg/caller/caller.go`](../../../client/pkg/caller/caller.go) | 99 | `2a2c5e4ff41308fde0cca63e39b30ed20625bdc87c860ed2efeb183fa67f6660` |
| [`client/pkg/caller/caller_test.go`](../../../client/pkg/caller/caller_test.go) | 99 | `d256d5b16dd09b5a83d9aac07c158ede77400d80a1d26a52e12b995b216eafa7` |
| [`client/pkg/circuitbreaker/circuit_breaker.go`](../../../client/pkg/circuitbreaker/circuit_breaker.go) | 99 | `f5364cc36beec2eb3921520ba33d430642d011316fad598b080b011ba63dbb92` |
| [`client/pkg/circuitbreaker/circuit_breaker_test.go`](../../../client/pkg/circuitbreaker/circuit_breaker_test.go) | 99 | `4d1f49bda54ee024329c6a3bd088fe3f997519262ef6c89b0a94671170128373` |
| [`client/pkg/connectionctx/manager.go`](../../../client/pkg/connectionctx/manager.go) | 99 | `92778a87fb3af520cb4c5fd1161a2438f02ee8151ab52c242af0e7ad011c72c4` |
| [`client/pkg/connectionctx/manager_test.go`](../../../client/pkg/connectionctx/manager_test.go) | 99 | `e5064d22a3ccbd771a7dc30858c2f24e74070fd615b888de2f92e51bea3ff00e` |
| [`client/pkg/deadline/watcher.go`](../../../client/pkg/deadline/watcher.go) | 99 | `949508a4a92b190d31c744cf02ff6786a8e5278ef0883b62b58a5cb5f7c4bfbe` |
| [`client/pkg/deadline/watcher_test.go`](../../../client/pkg/deadline/watcher_test.go) | 99 | `dd59310dca86588c9e503469ba049272dce30306f4070fd692bbc72399e361f6` |
| [`client/pkg/retry/backoff.go`](../../../client/pkg/retry/backoff.go) | 99 | `7e8933434ddf483e6a8777c22aebfdb2aa619e690e9bc49c8603ff0dfcc02c80` |
| [`client/pkg/retry/backoff_test.go`](../../../client/pkg/retry/backoff_test.go) | 99 | `8bad4a39dac523558691b3f56cebf58e1c118496c8dff5f2284515d755530f32` |
| [`client/pkg/retry/interval_retry.go`](../../../client/pkg/retry/interval_retry.go) | 99 | `4df997fea114ee60cfabd333c04f7bc58913c6c5fb05449de554643bb456bf86` |
| [`client/pkg/utils/grpcutil/grpcutil.go`](../../../client/pkg/utils/grpcutil/grpcutil.go) | 99 | `b28129fda2a65b18e5ad394e1dee5a9074cd04b5a038b879bf12ffa85e935bc4` |
| [`client/pkg/utils/grpcutil/grpcutil_test.go`](../../../client/pkg/utils/grpcutil/grpcutil_test.go) | 99 | `b38aabf7c547474c7b16341dff203de1a6a2db6ceb4726372f869f5bd08bf3a6` |
| [`client/pkg/utils/testutil/check_env_dummy.go`](../../../client/pkg/utils/testutil/check_env_dummy.go) | 99 | `c24555281422963888221434be397b9909f799ce7dd1f4a2a4db218b65437cc2` |
| [`client/pkg/utils/testutil/check_env_linux.go`](../../../client/pkg/utils/testutil/check_env_linux.go) | 99 | `400885eecc41a9badf71bd8415ce72afd03e91226347d6283d1328cdf0c7cade` |
| [`client/pkg/utils/testutil/tempurl.go`](../../../client/pkg/utils/testutil/tempurl.go) | 99 | `04ce093e14184b51d0bb0aa3b010802d7e6f895891cc841209ab98c9930b42e2` |
| [`client/pkg/utils/testutil/testutil.go`](../../../client/pkg/utils/testutil/testutil.go) | 99 | `5253e9c3618e83d315b7ea9032270f806bb4b9261c7b791afd95b3d421790fbd` |
| [`client/pkg/utils/timerutil/pool.go`](../../../client/pkg/utils/timerutil/pool.go) | 99 | `cc052c9e58336eb3073beafd65e7fd74e355f717bc05feabba91189a1a52c7b6` |
| [`client/pkg/utils/timerutil/pool_test.go`](../../../client/pkg/utils/timerutil/pool_test.go) | 99 | `6e14c6b7b3fe9d49a141b32ea4f7e117004136a3473a539a1d61254115fcd7cd` |
| [`client/pkg/utils/tlsutil/OWNERS`](../../../client/pkg/utils/tlsutil/OWNERS) | 99 | `e47f53005bbfb0c154640e16ad3a873314ccb16adae109d3462935a471830309` |
| [`client/pkg/utils/tlsutil/tlsconfig.go`](../../../client/pkg/utils/tlsutil/tlsconfig.go) | 99 | `8c63030430b0d2287700e5ff024c7db5adb4d595460d15a488a94ed7a7103d00` |
| [`client/pkg/utils/tlsutil/tlsutil.go`](../../../client/pkg/utils/tlsutil/tlsutil.go) | 99 | `a836a7c3d66afcb069aaacab01c3684cbfefbadbcbd84aaa5a8167c9d6f7ebe1` |
| [`client/pkg/utils/tlsutil/url.go`](../../../client/pkg/utils/tlsutil/url.go) | 99 | `dc3f37931e4a36074615b4f64fcc901c058cc39b89d6ad0eece8caa51a951078` |
| [`client/pkg/utils/tsoutil/tsoutil.go`](../../../client/pkg/utils/tsoutil/tsoutil.go) | 99 | `67c5b0d93c59d77c573b4837aaf4a98c1fa97f00cd126828147840152c985a0a` |
| [`client/resource_group/controller/OWNERS`](../../../client/resource_group/controller/OWNERS) | 99 | `0e5bc2ebeac2e7cfb624c6995a3b878f4172de43203000f8098db005d65a2243` |
| [`client/resource_group/controller/config.go`](../../../client/resource_group/controller/config.go) | 99 | `3f339a7c178e25137103338c0dcc4d38df9c8699d88629d4fec1e81d8cf4f608` |
| [`client/resource_group/controller/global_controller.go`](../../../client/resource_group/controller/global_controller.go) | 99 | `9709bd33d0ed6a8078551ce099ee24575f9d0cf6e032724f97fc9dd1d520acd9` |
| [`client/resource_group/controller/global_controller_test.go`](../../../client/resource_group/controller/global_controller_test.go) | 99 | `4cf947ea6a0dce9365d688e66862e6f3eb1ece3809f324ab75b8a676f2fbf5af` |
| [`client/resource_group/controller/group_controller.go`](../../../client/resource_group/controller/group_controller.go) | 99 | `e751ce097735d4446f77e0c2fdc8c40724ec4b5ac703fd7f27026f5be264f855` |
| [`client/resource_group/controller/group_controller_test.go`](../../../client/resource_group/controller/group_controller_test.go) | 99 | `99e1418f596cc7a0b58bf84961eb0ce01846b1a052ab620f45dede7fe9565aaa` |
| [`client/resource_group/controller/limiter.go`](../../../client/resource_group/controller/limiter.go) | 99 | `3e08e99b23b8070e14b21f6c6117701f3604fdea1da66144269452137e24bc0e` |
| [`client/resource_group/controller/limiter_test.go`](../../../client/resource_group/controller/limiter_test.go) | 99 | `c0d5ab4d7c46817f9bcad3040f6d57ff086a9ce41cff32d94752ecec153b9f5e` |
| [`client/resource_group/controller/metrics/metrics.go`](../../../client/resource_group/controller/metrics/metrics.go) | 99 | `f45e77d16ca904ac6dd34eb5e91cdf7d966f57d6d6c0efe141441b2e6e7053dd` |
| [`client/resource_group/controller/model.go`](../../../client/resource_group/controller/model.go) | 99 | `5b2a2619dc4f88413ee88100499683fb3914e1d4d9f8dbec2c861ac1b3227b02` |
| [`client/resource_group/controller/model_test.go`](../../../client/resource_group/controller/model_test.go) | 99 | `96654f42f8408957d528f2976cd60a972608da0d1f3d9c19d38b6b5e0f67359d` |
| [`client/resource_group/controller/request_source_metrics_test.go`](../../../client/resource_group/controller/request_source_metrics_test.go) | 99 | `50a307663028d51d1f8c8f0e0e2efeef2f8eb98f250a4ab4026b69df2edc23cb` |
| [`client/resource_group/controller/testutil.go`](../../../client/resource_group/controller/testutil.go) | 99 | `ec6229e1dcb1b593d1c64584d871579665329bd1eff66a68dc4896893d59666c` |
| [`client/resource_group/controller/util.go`](../../../client/resource_group/controller/util.go) | 99 | `75514dae232a750e35f66da4c05ba4431395fc1f587c4ec545a4074433c83844` |
| [`client/resource_group/controller/util_test.go`](../../../client/resource_group/controller/util_test.go) | 99 | `c8f15acc826f0988c7b53179b5c046a22b80ec525ed83629150586991879895a` |
| [`client/resource_manager_client.go`](../../../client/resource_manager_client.go) | 99 | `fbb92087e32420aad9c82ba9f15eac7b052bcaa068257481b6e3fa79204042f5` |
| [`client/resource_manager_client_test.go`](../../../client/resource_manager_client_test.go) | 99 | `232e029b4e5286f3fab26807e5719622ddd47e85071f1cd19db6ef17768de6f7` |
| [`client/servicediscovery/callbacks.go`](../../../client/servicediscovery/callbacks.go) | 99 | `5f927301227919e13a6c30a0466b749e25765e85e8ca69d73fa131972b318f25` |
| [`client/servicediscovery/mock_service_discovery.go`](../../../client/servicediscovery/mock_service_discovery.go) | 99 | `bb36615732f3228f1078b74c347124837651fe81b5173144d4de456abc69e3a1` |
| [`client/servicediscovery/resource_manager_service_discovery.go`](../../../client/servicediscovery/resource_manager_service_discovery.go) | 99 | `7483f9b4dd2043089ea1e3ae502d64f4126a46da5c079a5db41294f8fea573a1` |
| [`client/servicediscovery/resource_manager_service_discovery_test.go`](../../../client/servicediscovery/resource_manager_service_discovery_test.go) | 99 | `1172beb5268c5c13bab3f9825bacfa97ce4bf8203c1a02b3367cef06e896687e` |
| [`client/servicediscovery/router_service_discovery.go`](../../../client/servicediscovery/router_service_discovery.go) | 99 | `09a3f34fec352c4ae1032d211438140fef1a946b9168723bcc713d64febd777e` |
| [`client/servicediscovery/service_discovery.go`](../../../client/servicediscovery/service_discovery.go) | 99 | `620648327631b098c3a6a840648e0dc6c071ce3a6f5d5c9a8ed98d05ebad3bd9` |
| [`client/servicediscovery/service_discovery_test.go`](../../../client/servicediscovery/service_discovery_test.go) | 99 | `26522fcc921e239c3d04d92e63871c3849f41f40da46d1bd934be3cc9bfbffc9` |
| [`client/servicediscovery/tso_service_discovery.go`](../../../client/servicediscovery/tso_service_discovery.go) | 99 | `8947f1332fe40283e360608f3f49450d39ea2797beb56da018957ac8456d35de` |
| [`client/servicediscovery/tso_service_discovery_test.go`](../../../client/servicediscovery/tso_service_discovery_test.go) | 99 | `a2379d4732fddd244d08d0c8e38e2e0f928d937ba2fd830ac5461c9091aa94a9` |
| [`cmd/pd-server/Cargo.toml`](../../../cmd/pd-server/Cargo.toml) | 102 | `255faa82274f55b315a742a9439c00084182ee14df12050c3d71c9b8befcb891` |
| [`cmd/pd-server/bin_main.rs`](../../../cmd/pd-server/bin_main.rs) | 102 | `73a0908402dbee37f9745dc0b44bd61380defb176e5c1f8e660d2d12a94aa92c` |
| [`cmd/pd-server/lib.rs`](../../../cmd/pd-server/lib.rs) | 102 | `ac786710db8a0e1eb5dd4071f355344f2bea1acf66c8a569d6403031677fdb30` |
| [`cmd/pd-server/main.go`](../../../cmd/pd-server/main.go) | 3,90,91 | `00091e020c70f2a206e3dbe34fff924e0eb17da0a9a700121ba1b2d73318565d` |
| [`cmd/pd-server/main.rs`](../../../cmd/pd-server/main.rs) | 90,91,92,97 | `44e254aafeb76f5de31d874234792499ae49ebaddb1ca3d6e292ed0ef77ba970` |
| [`cmd/pd-server/main_test.rs`](../../../cmd/pd-server/main_test.rs) | 97 | `7cfffce64d3f51f76db02942d26d3db670ae0f76e9041ba53d84afcb9b7c5a89` |
| [`docs/rust/architecture.md`](../../../docs/rust/architecture.md) | 96 | `3cb14bc9fcc037aaad34cf91ba7ade7343ae6a55e3325d2bbe2e8778e7ff8bef` |
| [`docs/rust/dependencies.md`](../../../docs/rust/dependencies.md) | 102 | `2e88d1d7d265b9e9281063d09f89e1a561d045c81015fb5c3d64692403e22343` |
| [`docs/rust/porting-guide.md`](../../../docs/rust/porting-guide.md) | 102 | `8bfe9b1102e35a783402c7347e1e42ebf68581fa586736cf783cf32967c8e807` |
| [`docs/rust/runtime.md`](../../../docs/rust/runtime.md) | 102 | `1712f9df133daad7afbaae4e9a6fe9152404142dd2ab7974d8ecaf72aca78c97` |
| [`docs/rust/schedule-execution/logs/task_156/audit.json`](../../../docs/rust/schedule-execution/logs/task_156/audit.json) | 102 | `dee6528a7771e1c87eb741061cbe053377e262bf1f6515f541b5372dbd96d1c8` |
| [`docs/rust/schedule-execution/logs/task_156/dependency-audit.json`](../../../docs/rust/schedule-execution/logs/task_156/dependency-audit.json) | 102 | `7496aeed3260eaabd2892703588beb900fdeea78fd195f804c40280ace978956` |
| [`docs/rust/schedule-execution/logs/task_156/normal-tree-0.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/normal-tree-0.log.gz) | 102 | `22b042852f2742d03d0b5839e24f4cfcb1ea3157e67e186ed87f75b12f539c96` |
| [`docs/rust/schedule-execution/logs/task_156/normal-tree.json`](../../../docs/rust/schedule-execution/logs/task_156/normal-tree.json) | 102 | `af2f452c264e10036c5f3df48311c04e283bd8e00c022ce231b824604b467927` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-0.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-0.log.gz) | 102 | `5b86412802ffc8d14e8cf895dc9b65c51d3a7238fc519c952ae69d348e821bdc` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-1.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-1.log.gz) | 102 | `bf99c88df5efeea1bb43ddf84e757a307301e17f6e299dad69a808738849790b` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-10.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-10.log.gz) | 102 | `50de671caf4c2d44a32810cf181848a1524c8c11d3dd9ecee2c1007a35a75f81` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-2.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-2.log.gz) | 102 | `cdcc1a8997bfe061a61e8f491843485ceee6a5d2c5a1aba36f4df0c0dc548557` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-3.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-3.log.gz) | 102 | `0c2462f83edf576b2fa9b57f36054ed47359e38b90b53c3c66f3a080311070ca` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-4.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-4.log.gz) | 102 | `f72dfaa05f3158aa47d683dfd1f65c3eea5d65cb85677ee0b51d59fc3ac1d65f` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-5.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-5.log.gz) | 102 | `0e1fe4c76fede21a5a274dc400a3211eb217cc61241e872dc70f17f0a978be33` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-6.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-6.log.gz) | 102 | `194872693a4c842ba490729ad2f12183fee966e1b74bc8f047d913162a53e240` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-7.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-7.log.gz) | 102 | `b1cfb46d7dab4bc322749182c96b15b4a2d6ab305f4c077aedf4a1ddcc5b0b5b` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-8.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-8.log.gz) | 102 | `95fa04e3c47a48d273bd897b4f372411942f49a55acaf54d0df87a5ee9ae0995` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-9.log.gz`](../../../docs/rust/schedule-execution/logs/task_156/repaired-9.log.gz) | 102 | `b359a3318f30b34606a1900eba47e087910cac5ca63a3170aeb98b540e3ba9c8` |
| [`docs/rust/schedule-execution/logs/task_156/repaired-provenance.json`](../../../docs/rust/schedule-execution/logs/task_156/repaired-provenance.json) | 102 | `de1b88e21f13b7ef34531c9d9d57cd6740abad1695aa2532b3fa21bf0a334432` |
| [`docs/rust/schedule-execution/logs/task_156/repaired.json`](../../../docs/rust/schedule-execution/logs/task_156/repaired.json) | 102 | `42e4add99ac6a05bbd671ec6de273fd81c7fc44245369ecb61b868b95ccc10a2` |
| [`docs/rust/schedule-execution/logs/task_156/verified-summary.json`](../../../docs/rust/schedule-execution/logs/task_156/verified-summary.json) | 102 | `5fdd64018e11f2d57bbafcf9c60ca0357710a49ef3f257c46c61a87360fdae9b` |
| [`docs/rust/schedule-execution/oracle-index.json`](../../../docs/rust/schedule-execution/oracle-index.json) | 102 | `2d1a930742291e7047a864aa2246e7e22c612cb6583249b7edb42945b494b450` |
| [`docs/rust/schedule-execution/source-inventory.json`](../../../docs/rust/schedule-execution/source-inventory.json) | 102 | `194c90f160c24dda9e688b75ce3114333919fa36a7c0d78d49b1409c5eb286bc` |
| [`docs/rust/schedule-execution/task-156.md`](../../../docs/rust/schedule-execution/task-156.md) | 1 | `536352a1365d5d86a884fe09e0c92bc8998ee42e964a640bbc00616ec5030a87` |
| [`pkg/audit/audit.go`](../../../pkg/audit/audit.go) | 81 | `088f70fe9fe18b0ef5d99ad41160fe399b7d0b1d3330eefcfc300f174f59960d` |
| [`pkg/audit/audit_test.go`](../../../pkg/audit/audit_test.go) | 81 | `e28bb19d7759a66a2e4d2067b9fed6aef3d629ee10353ad69c177261452dc24b` |
| [`pkg/basicserver/basic_server.go`](../../../pkg/basicserver/basic_server.go) | 3,43,93 | `52b69ee57a97d19f4f3e10593a78d024b642b1786b373bf1d7ffecf20797fae5` |
| [`pkg/basicserver/metrics.go`](../../../pkg/basicserver/metrics.go) | 3,43,86 | `31c02659be8ef323598358d00f7bc931fe3f41a40216ec012385ce5262681be0` |
| [`pkg/core/Cargo.toml`](../../../pkg/core/Cargo.toml) | 24 | `26aa98bfac538dafd62482995a731f01a0052aabf31c20b11e9ce6a2c2f45f81` |
| [`pkg/core/basic_cluster.go`](../../../pkg/core/basic_cluster.go) | 24 | `a9dc7beaa0c8ce2a8c00aa9d346bf21f242af43bcb4c670fec025f0313b010a0` |
| [`pkg/core/basic_cluster.rs`](../../../pkg/core/basic_cluster.rs) | 24 | `860df54918828fd6718a940ae4eb62165fc4267a9fecbcad1d4f4657d9ce2a3c` |
| [`pkg/core/basic_cluster_rates.rs`](../../../pkg/core/basic_cluster_rates.rs) | 24 | `16835572186049d175e24696d6413471d1e9cb0cad33e8f3b0a5fc3e5a13af7d` |
| [`pkg/core/basic_cluster_rates_test.rs`](../../../pkg/core/basic_cluster_rates_test.rs) | 24 | `f55d2f7f75757e4acfbb3a9f5e3ab1bf278c7da4d5ef014c200003927b03a917` |
| [`pkg/core/basic_cluster_test.rs`](../../../pkg/core/basic_cluster_test.rs) | 24 | `f1417eaee6021accecb294a743b61c36a441c699e4f92d3de0eeaaca30fc723c` |
| [`pkg/core/constant/Cargo.toml`](../../../pkg/core/constant/Cargo.toml) | 24 | `4b5f6dff593f3e15dafe6820b88df4d8bb7c40dc0392185a15b6add5448a9eaf` |
| [`pkg/core/constant/kind.go`](../../../pkg/core/constant/kind.go) | 24 | `8779993000dc309b7f3bf83ce96dfd3f7c1df0c314e04301ebe8876d8cbca908` |
| [`pkg/core/constant/kind.rs`](../../../pkg/core/constant/kind.rs) | 24 | `6a04c0af988a6f81fbab7c8b1d2277426f9d6216ec7eb92711e855f51f2c8202` |
| [`pkg/core/constant/kind_test.rs`](../../../pkg/core/constant/kind_test.rs) | 24 | `13069d11d5d6457091124285290c0c0de65f9812572d845208abb71bf5a3db94` |
| [`pkg/core/context.go`](../../../pkg/core/context.go) | 24 | `64210382489f390e2b4807f80838842b6e380df3c0e50c599ed86e889ed6be23` |
| [`pkg/core/factory.go`](../../../pkg/core/factory.go) | 24 | `e89be66bd0f03a12f9ed5669b7745a022c6d58ced8d8bea4efe89de93c0770f0` |
| [`pkg/core/factory_test.go`](../../../pkg/core/factory_test.go) | 24 | `29bfbd637d378f53a83c12b0023689f9038d18aef0363a7a100fd510d0fe8922` |
| [`pkg/core/lib.rs`](../../../pkg/core/lib.rs) | 24 | `9c499448f33cc8337b9c0c17a178ec80b256d335a98b38837fbe000dfe5b54f2` |
| [`pkg/core/metrics.go`](../../../pkg/core/metrics.go) | 24 | `13fd85dba12904604493abeb2e34b32f7208f6b08ea393eec99368142fc63a7a` |
| [`pkg/core/metrics_test.go`](../../../pkg/core/metrics_test.go) | 24 | `389d3c3ec6081dd177d14011a140ea5bc097ea11d25cf779b267dc0fadb0b2b2` |
| [`pkg/core/peer.go`](../../../pkg/core/peer.go) | 24 | `cc1895b62828225902a4e9d39febd9f019dbb5fd0b5bcfea30d2e10807ec153d` |
| [`pkg/core/peer.rs`](../../../pkg/core/peer.rs) | 24 | `861c48e9a0cbdd34e17cf7500835d35a98cfc5bccc0637a7830ef4ffa9b74c1c` |
| [`pkg/core/peer_test.rs`](../../../pkg/core/peer_test.rs) | 24 | `3e402228d34ead910ff12da4e0a5274ffb841866f3b396ded23a6558237b22ef` |
| [`pkg/core/rangetree/Cargo.toml`](../../../pkg/core/rangetree/Cargo.toml) | 24 | `6fcec7869d1e8a8e6e24e9650830d6f8342a2f0d531466f75d8b7f7ec930b7c5` |
| [`pkg/core/rangetree/lib.rs`](../../../pkg/core/rangetree/lib.rs) | 24 | `03b3a07ec1f3d0453276aeb1eee8d71be9bbf2b9389855eb7bce93d7e3cb640d` |
| [`pkg/core/rangetree/lib.rs.md`](../../../pkg/core/rangetree/lib.rs.md) | 24 | `251e71ad80af3c2945e07adc1875f43421e0d6f93d04313a4700723ed0a1daac` |
| [`pkg/core/rangetree/range_tree.go`](../../../pkg/core/rangetree/range_tree.go) | 24 | `6e83fc697d8f63b8424d70960c883943d7a181d4b6d8232ec54e4b2e569d5298` |
| [`pkg/core/rangetree/range_tree_test.go`](../../../pkg/core/rangetree/range_tree_test.go) | 24 | `64fb0195980314b96a1a676fe361b67f434b9e35050d7d4fbe373339fa3629e9` |
| [`pkg/core/rangetree/range_tree_test.rs`](../../../pkg/core/rangetree/range_tree_test.rs) | 24 | `e6ccffca3f209493155f66cb45b2363d57804c367a7b950b88863c19f2e6b733` |
| [`pkg/core/region.go`](../../../pkg/core/region.go) | 24 | `be6ed0dc77537d62926d21d9e4eef080ea0dbcc7dc97efb4dc26269a172f99d8` |
| [`pkg/core/region.rs`](../../../pkg/core/region.rs) | 24 | `58a71f2b409142bfc3d0ab27afd70f2a7d7440f4565657068529a2d22cada46a` |
| [`pkg/core/region_buckets.rs`](../../../pkg/core/region_buckets.rs) | 24 | `663fd1f817d33b4ddec007303e9e28bf43b5701c0e36b15e1d51d70ffbc2960b` |
| [`pkg/core/region_buckets_test.rs`](../../../pkg/core/region_buckets_test.rs) | 24 | `5f82c0e5c18bbf953093c2ed3d1ac1995ac7c7299692158d73330d7b19e46206` |
| [`pkg/core/region_clone.rs`](../../../pkg/core/region_clone.rs) | 24 | `d3f038e593709f8105588bc459c2dc8237906b0f2e4bb4a43b28292d36f31a80` |
| [`pkg/core/region_clone.rs.md`](../../../pkg/core/region_clone.rs.md) | 24 | `532e7284c9c426475b162624e6feb73272c4efc73c943e67ca28905a70a35fc0` |
| [`pkg/core/region_clone_test.rs`](../../../pkg/core/region_clone_test.rs) | 24 | `2699a47e745b6273cde83a22a7708be3d84fd433a5698eca4931f103c9edc1ea` |
| [`pkg/core/region_flow.rs`](../../../pkg/core/region_flow.rs) | 24 | `865df29e0368a3b50e573411c849f536f1551f31fb7fa445ddecd2b9b48b15ab` |
| [`pkg/core/region_flow.rs.md`](../../../pkg/core/region_flow.rs.md) | 24 | `88cfd6091f957ffae2fd5a6494de7d493b4de93336462019e09f15930a29f687` |
| [`pkg/core/region_flow_test.rs`](../../../pkg/core/region_flow_test.rs) | 24 | `05d3eee60bb4303f01778fd63c38486da614970e0a6d753468abe06c3bd49e44` |
| [`pkg/core/region_format.rs`](../../../pkg/core/region_format.rs) | 24 | `c71c8a3957132c66e69e466a0f4b495ef88684a243922a4b1827477040ffa9dd` |
| [`pkg/core/region_format_test.rs`](../../../pkg/core/region_format_test.rs) | 24 | `83599bef5179e04d9b1a9cf03eefe2b67e6c4bbeb714c623e0f8f8976a06a79d` |
| [`pkg/core/region_guide.rs`](../../../pkg/core/region_guide.rs) | 24 | `1139e99cc04fafc812747126a82b4aa2b7839b52394a947643d6d7c9d56db8a0` |
| [`pkg/core/region_guide_test.rs`](../../../pkg/core/region_guide_test.rs) | 24 | `eea0e675b89cf5497358c17318519946b64330eadf08926d1bd333d334eb2251` |
| [`pkg/core/region_inherit.rs`](../../../pkg/core/region_inherit.rs) | 24 | `bf6039f276ca62bc26aa7da62ac900c7bcfb7cdde686758289797434ce3dcb7c` |
| [`pkg/core/region_inherit.rs.md`](../../../pkg/core/region_inherit.rs.md) | 24 | `36e93655c958c7a5e9e37f4a44707b5cff7425e4d5039ef0edc567dd2795ff6a` |
| [`pkg/core/region_inherit_test.rs`](../../../pkg/core/region_inherit_test.rs) | 24 | `e2ace819af851389ebe6201bcf7597491cbf9adf2bc3ad05f034f39ea1a69edd` |
| [`pkg/core/region_merge.rs`](../../../pkg/core/region_merge.rs) | 24 | `66e85b22e25a0b152a18ca703aa9d6171b261cf0efcde75f3d802c36fa132d5c` |
| [`pkg/core/region_merge_policy.rs`](../../../pkg/core/region_merge_policy.rs) | 24 | `22f408b22d5053d54b98c7182ebb629a7c6bc2f114b84d7f9d6eca5c9155fa2f` |
| [`pkg/core/region_merge_policy_test.rs`](../../../pkg/core/region_merge_policy_test.rs) | 24 | `75f0a47fc8fb7509ad4c8c1b5f73d767f57934e5bb954775ab21de04cbe24b3b` |
| [`pkg/core/region_merge_test.rs`](../../../pkg/core/region_merge_test.rs) | 24 | `37051a8b47672bd9ac4c6a7fabfcc6dc1d194949f5a06bae5d1c239734180f00` |
| [`pkg/core/region_meta.rs`](../../../pkg/core/region_meta.rs) | 24 | `ed11ed2fc6b0c574c5ba6dd373736dc00cd7518a7e0c7e39c56c9a4ea02c030b` |
| [`pkg/core/region_meta_test.rs`](../../../pkg/core/region_meta_test.rs) | 24 | `43f8644a9d5cc73a27b6702f9f545119eea25e64d99b01b0d788ae7f14a29885` |
| [`pkg/core/region_option.go`](../../../pkg/core/region_option.go) | 24 | `275af41444105d04b6526bd401892d81f4babdefb8a5b31f86088d53dc67a8f9` |
| [`pkg/core/region_query.rs`](../../../pkg/core/region_query.rs) | 24 | `a08b8c4e64a914d9658a515225bf3ba543ebd01cf1125fccad8911789fb2b469` |
| [`pkg/core/region_query.rs.md`](../../../pkg/core/region_query.rs.md) | 24 | `7768e506e51455a4acf73b37b9eb958558a708d4defc34be084ba13f346818f1` |
| [`pkg/core/region_query_test.rs`](../../../pkg/core/region_query_test.rs) | 24 | `b4c9856a35fe4259100b777fc280ef8181c5d3a728f3dc051f6328300fc5b2e5` |
| [`pkg/core/region_split.rs`](../../../pkg/core/region_split.rs) | 24 | `044e13ffe17f057321185e6afe686bbe7cc6aaf1da1844b9d04436f32ef66dde` |
| [`pkg/core/region_split_test.rs`](../../../pkg/core/region_split_test.rs) | 24 | `eb86b47542f21eee60a7d10e4d7ca36ca3df230a1b4b86a5cc886059313bfe5e` |
| [`pkg/core/region_stale.rs`](../../../pkg/core/region_stale.rs) | 24 | `f3ef917759ba382f12e008061a4df9edb50e5b35e3cb373f330689d1777c5ee8` |
| [`pkg/core/region_stale_test.rs`](../../../pkg/core/region_stale_test.rs) | 24 | `6c9427d683cf81b50f9a3f147282dd6a293b8ce7bc766871311a622e0272a0ed` |
| [`pkg/core/region_test.go`](../../../pkg/core/region_test.go) | 24 | `f555c2ca97608c0925dc4b945ca95f8807f7c76b29ed9ed2896988933338cbbd` |
| [`pkg/core/region_test.rs`](../../../pkg/core/region_test.rs) | 24 | `97f1dd4b497629ba59ca6da20f0922981732f13746272403bf0d49d6b6e5ffee` |
| [`pkg/core/region_tree.go`](../../../pkg/core/region_tree.go) | 24 | `a17ea7b65dfe4ca3e5dc01a88d6b8fefb85774fec4b45d69429eccbb7d1e1718` |
| [`pkg/core/region_tree.rs`](../../../pkg/core/region_tree.rs) | 24 | `b2586f4a93c5aea73a22fe209a102313c90d2d09c0aceac12319d1a063f8d118` |
| [`pkg/core/region_tree_random.rs`](../../../pkg/core/region_tree_random.rs) | 24 | `6fb2653187e308a021366bb8938c1d613f28838771a1abe72f986a01f6a69fbb` |
| [`pkg/core/region_tree_random_test.rs`](../../../pkg/core/region_tree_random_test.rs) | 24 | `401df63059b15ce2f4bad1611613edd755382fdcdd3647585256132ade2df0b7` |
| [`pkg/core/region_tree_scan.rs`](../../../pkg/core/region_tree_scan.rs) | 24 | `3d575026d88f76dda74a30b616aee16d0bba7d148ff24e1379d8c4883a41afcd` |
| [`pkg/core/region_tree_scan_test.rs`](../../../pkg/core/region_tree_scan_test.rs) | 24 | `4b41032585389b9ffed61ebcd18a1607439e35f33c6c05b088652ce715cc9091` |
| [`pkg/core/region_tree_stats.rs`](../../../pkg/core/region_tree_stats.rs) | 24 | `011b3aaf440b07caccdaa3077855bac514ddcba2d9258eeb1a98be610daa1cab` |
| [`pkg/core/region_tree_stats_test.rs`](../../../pkg/core/region_tree_stats_test.rs) | 24 | `3db4c5e4df7acf7e8c469c6ddb9ce68864ba5d214d31ab1688e2701bd85438ed` |
| [`pkg/core/region_tree_test.go`](../../../pkg/core/region_tree_test.go) | 24 | `ed94085bfbf565766716f212624b2e4f5a232a90fb2d8ca1a185374b015f06cf` |
| [`pkg/core/region_tree_test.rs`](../../../pkg/core/region_tree_test.rs) | 24 | `936d8807d2bb8504fe5a7c872b112ee8c0a7db5f6653bd2ef6d7d761c848c645` |
| [`pkg/core/region_tree_update.rs`](../../../pkg/core/region_tree_update.rs) | 24 | `c2b5a5f485f2c5512665b24c095003d6e30ef587d65150d925a480f891f0f80e` |
| [`pkg/core/region_tree_update_test.rs`](../../../pkg/core/region_tree_update_test.rs) | 24 | `774be676a96d21a862f47908ed739e867d00d4bccacbc2ebda8d632ce1312f1e` |
| [`pkg/core/regions.rs`](../../../pkg/core/regions.rs) | 24 | `b634127113b8e7127dea0abca8c7968f1271309f683cc435a037aa0f3954acaa` |
| [`pkg/core/regions_atomic.rs`](../../../pkg/core/regions_atomic.rs) | 24 | `17bd71eb469152994028044dcbc65d074bbd101546f753e24d9cfaa305d4b0d2` |
| [`pkg/core/regions_atomic.rs.md`](../../../pkg/core/regions_atomic.rs.md) | 24 | `68fad463c3a67917691d71665d645a75f7169ad9e5cddfe92b32d29923241a95` |
| [`pkg/core/regions_atomic_test.rs`](../../../pkg/core/regions_atomic_test.rs) | 24 | `a17254ca08c8ce309e70355a82fd38ae039b582eb3ad2d31205cab61288bc83f` |
| [`pkg/core/regions_batch_scan.rs`](../../../pkg/core/regions_batch_scan.rs) | 24 | `6b4d6c6098e61ca09152b0b3b1d1236fe156ca7284efb004691f161cf8f8463c` |
| [`pkg/core/regions_batch_scan_test.rs`](../../../pkg/core/regions_batch_scan_test.rs) | 24 | `fb8b6850a5190260ba76432e8228210be16d0f0209b5e6d4f2159cba682f48fb` |
| [`pkg/core/regions_query.rs`](../../../pkg/core/regions_query.rs) | 24 | `2754d8f1405a38c788c84ba2267a723895c78f0c89450efebda409d01fdb8fe4` |
| [`pkg/core/regions_query_test.rs`](../../../pkg/core/regions_query_test.rs) | 24 | `b2ce583957f5248152cf408e70a410492273741f02056adebff23256ffeae8d2` |
| [`pkg/core/regions_random.rs`](../../../pkg/core/regions_random.rs) | 24 | `0d03821d3480f03901fc512bc23e45e96415e9f86c135ee6e1fb4d4dc9c15150` |
| [`pkg/core/regions_random_test.rs`](../../../pkg/core/regions_random_test.rs) | 24 | `ac8101ae1a82bbccded10000eac638efed203d1bee355e073e9b922394150099` |
| [`pkg/core/regions_range_stats.rs`](../../../pkg/core/regions_range_stats.rs) | 24 | `4d0a91d11bc3f8e3bc7d64bc3ab86d161d5069f292462d5943282bfd8622f3f8` |
| [`pkg/core/regions_range_stats_test.rs`](../../../pkg/core/regions_range_stats_test.rs) | 24 | `55362b4488424396c9a4c9b19cd8fd055045a2183f3b7368cb7a12f6ea85b108` |
| [`pkg/core/regions_reset.rs`](../../../pkg/core/regions_reset.rs) | 24 | `95f614577ea693645ecae97aa32dbe3c88af106e20e0fc5b80e3168018909b19` |
| [`pkg/core/regions_reset_test.rs`](../../../pkg/core/regions_reset_test.rs) | 24 | `7291ef8ab3cc2603f8c640252c56ac9f1d501ae2e58b89e4ce1cb74064e25f0b` |
| [`pkg/core/regions_scan.rs`](../../../pkg/core/regions_scan.rs) | 24 | `f76bac4bc0ff982e2df8219828e99f2a67841ad6e49a90863168d799598132b4` |
| [`pkg/core/regions_scan_test.rs`](../../../pkg/core/regions_scan_test.rs) | 24 | `61686ddfb3cac572fd2addb01949d947b8d1795e45e393320914c448c2059fe3` |
| [`pkg/core/regions_stats.rs`](../../../pkg/core/regions_stats.rs) | 24 | `6c2acf1038e9b6858367afe78a80833c3d079b400b30ce2ea6c5faa1984c41ed` |
| [`pkg/core/regions_stats_test.rs`](../../../pkg/core/regions_stats_test.rs) | 24 | `9ca83ebec8bd3ce0d11a14405499aae42fecdf4de5e6065e097e77ff7ab94c7c` |
| [`pkg/core/regions_subtree.rs`](../../../pkg/core/regions_subtree.rs) | 24 | `23e8e9dd2bfd8df0e1f1e0d6f0d3e933ba7f16db329a76c0f922089fa35075ba` |
| [`pkg/core/regions_subtree_order.rs`](../../../pkg/core/regions_subtree_order.rs) | 24 | `8bd1a2a65f8e326cf9b6a61faf94abdd7120dba237d03f1f2d6f4eebec551c72` |
| [`pkg/core/regions_subtree_order_test.rs`](../../../pkg/core/regions_subtree_order_test.rs) | 24 | `8aa8d6d6ad4fbf41319749c440b5143a0afa9f2b5ab1c93041f253143fc9d6a8` |
| [`pkg/core/regions_subtree_test.rs`](../../../pkg/core/regions_subtree_test.rs) | 24 | `f21240d72721c3734b456dfb50a618ed8b9a2af91a0d45964276c506ee4d3400` |
| [`pkg/core/regions_test.rs`](../../../pkg/core/regions_test.rs) | 24 | `897a314297b6c8bd971f36889abd730efc7d937196165e5a8b0cc4d24b771ca4` |
| [`pkg/core/rust-fixtures/core_task_09.json`](../../../pkg/core/rust-fixtures/core_task_09.json) | 24 | `5eb12ba230c75394889965ff953925de59476ded904ba5ee73e038d402ac0bd6` |
| [`pkg/core/rust-fixtures/core_task_10.json`](../../../pkg/core/rust-fixtures/core_task_10.json) | 24 | `93b9c618473e28306484dbc944c37fa71b7e51bf61330ef3bd216136789704ac` |
| [`pkg/core/rust-fixtures/core_task_11.json`](../../../pkg/core/rust-fixtures/core_task_11.json) | 24 | `682813ab75c50d70fc93a4686b4bff2294bd51e2c235a5c969129fa4afaa8e40` |
| [`pkg/core/rust-fixtures/core_task_12.json`](../../../pkg/core/rust-fixtures/core_task_12.json) | 24 | `5dffd57def27643c736d5373ccd02a651dead28b95d69342b189d397f4565179` |
| [`pkg/core/rust-fixtures/core_task_13.json`](../../../pkg/core/rust-fixtures/core_task_13.json) | 24 | `4ff53a286508686d28c5cc60f6708cee2013f05ef5adf6d130f06322f4feeac4` |
| [`pkg/core/rust-fixtures/core_task_14.json`](../../../pkg/core/rust-fixtures/core_task_14.json) | 24 | `8f6cd7e7183ba82a76ee299d726250ded3b044a32205da35f87740701c05eed8` |
| [`pkg/core/rust-fixtures/core_task_15.json`](../../../pkg/core/rust-fixtures/core_task_15.json) | 24 | `812770cbc65c0c2e6d6057655c6cd360b96b38fdfceece4c652174914e105573` |
| [`pkg/core/rust-fixtures/core_task_16.json`](../../../pkg/core/rust-fixtures/core_task_16.json) | 24 | `70f9cd35313471a1dc2f7b28de783510c3120f63ed490296b1c560cc1ac1fc1f` |
| [`pkg/core/rust-fixtures/core_task_17.json`](../../../pkg/core/rust-fixtures/core_task_17.json) | 24 | `fbde10c887eecc1f4758103bcbd61a6a21f236b2823c0fd9129b22efc2e10acd` |
| [`pkg/core/rust-fixtures/core_task_18.json`](../../../pkg/core/rust-fixtures/core_task_18.json) | 24 | `bec5408b6f152a23d8d4211e64176c46f8e0d13d29eb9873669544eeaeaca50c` |
| [`pkg/core/rust-fixtures/core_task_19.json`](../../../pkg/core/rust-fixtures/core_task_19.json) | 24 | `356feff7a1e0535dfe069c5b9767741f0c6e51fd7ac90eb0bc8dd80771adcec8` |
| [`pkg/core/rust-fixtures/core_task_20.json`](../../../pkg/core/rust-fixtures/core_task_20.json) | 24 | `e1b118d550a63a5eba355f13c1db71aa1e9bc25b9c0bc3a2320fbbb524b00c8d` |
| [`pkg/core/rust-fixtures/core_task_21.json`](../../../pkg/core/rust-fixtures/core_task_21.json) | 24 | `5fa750e69175503d2ac9b2872ae060a941c82188e0f67c1f58c1ee69aee5b1f9` |
| [`pkg/core/rust-fixtures/core_task_22.json`](../../../pkg/core/rust-fixtures/core_task_22.json) | 24 | `76feec2ae0a2fae6ec597e8ddbe15935c26f40692baecf2ddfa628414ccb8c80` |
| [`pkg/core/rust-fixtures/core_task_23.json`](../../../pkg/core/rust-fixtures/core_task_23.json) | 24 | `ced31828aa92e71c0cb4f0371bbf775ee233c657018ebfb1cbd0f0ba42f5e475` |
| [`pkg/core/rust-fixtures/core_task_24.json`](../../../pkg/core/rust-fixtures/core_task_24.json) | 24 | `0dbd03ccc50a9160b38ca71759c081432ae748579e712a2c265f13e645a2c6c9` |
| [`pkg/core/rust-fixtures/core_task_25.json`](../../../pkg/core/rust-fixtures/core_task_25.json) | 24 | `f7b5b3eb1ad02fba8a956d1da6240cd2147a5925aaf80ce6d05f7d5989368224` |
| [`pkg/core/rust-fixtures/core_task_26.json`](../../../pkg/core/rust-fixtures/core_task_26.json) | 24 | `da08621d30d7504f7a324aaf2bda89e4a677607c1915f4459017083fe52324df` |
| [`pkg/core/rust-fixtures/core_task_27.json`](../../../pkg/core/rust-fixtures/core_task_27.json) | 24 | `464695c8330a45b00b26e1eabf482ef9b68a9fd7447c52f6c1d35e1b81b88e63` |
| [`pkg/core/rust-fixtures/core_task_28.json`](../../../pkg/core/rust-fixtures/core_task_28.json) | 24 | `cb91287291359c361080855b06dbd9dd665ed42dea3923ad8c479cf2208c7477` |
| [`pkg/core/rust-fixtures/core_task_29.json`](../../../pkg/core/rust-fixtures/core_task_29.json) | 24 | `f3521eddd5d2e82fe6d0ba0880c1f522afce13788a62fa793f2fd76ac10a234f` |
| [`pkg/core/rust-fixtures/core_task_30.json`](../../../pkg/core/rust-fixtures/core_task_30.json) | 24 | `169ff16ad4ac6ca3d0a54061da7c5808978c11cb763ee15cbc72acefb86a7c6d` |
| [`pkg/core/rust-fixtures/core_task_31.json`](../../../pkg/core/rust-fixtures/core_task_31.json) | 24 | `ef98442e94ebb82f509a170a6eb8f05f4c956488bc219446b8fc32c15df41502` |
| [`pkg/core/rust-fixtures/core_task_32.json`](../../../pkg/core/rust-fixtures/core_task_32.json) | 24 | `020f9092fb5541b90204ecb3be3c7c500f65c1575c8bbbcf00ef576a64b482bc` |
| [`pkg/core/rust-fixtures/core_task_33.json`](../../../pkg/core/rust-fixtures/core_task_33.json) | 24 | `3bd9e7fd2788034b78dc78ae15cceec24b8ca3829545bbf8b594d5b2b9791cf4` |
| [`pkg/core/rust-fixtures/core_task_34.json`](../../../pkg/core/rust-fixtures/core_task_34.json) | 24 | `18102e88708d5bfe7e9cb3b1eba0e5b078eb1f80db3a0966c56d476a65d201c1` |
| [`pkg/core/rust-fixtures/core_task_35.json`](../../../pkg/core/rust-fixtures/core_task_35.json) | 24 | `f56175a78e0f10e220f295a8d9f80bd9c5053033379c43e82e92c64c74e1f9fe` |
| [`pkg/core/rust-fixtures/core_task_36.json`](../../../pkg/core/rust-fixtures/core_task_36.json) | 24 | `547baa737333d9b8a7b91dafce2501c265e668c357a31e09a5be705f428c8ac5` |
| [`pkg/core/rust-fixtures/core_task_37.json`](../../../pkg/core/rust-fixtures/core_task_37.json) | 24 | `705e0fa5779f8c99c673df04e7f9987ba5ec9f3f270a28f4f98c15510e2ae5fc` |
| [`pkg/core/rust-fixtures/core_task_38.json`](../../../pkg/core/rust-fixtures/core_task_38.json) | 24 | `4ce21b4c3ba677651cadf0a60cac94a92bb9a98dd6cc9144c5a0de86e413ee7d` |
| [`pkg/core/rust-fixtures/core_task_39.json`](../../../pkg/core/rust-fixtures/core_task_39.json) | 24 | `9936e6c254bdc75049d50f6f183e9c70fd02f23d3447fa1a1733bbd2e2a01d53` |
| [`pkg/core/rust-fixtures/core_task_40.json`](../../../pkg/core/rust-fixtures/core_task_40.json) | 24 | `970b4cda227daf0b68e067fc04f2c6aef3b6556af8db5010f5b1f5bff286ef30` |
| [`pkg/core/rust-fixtures/core_task_41.json`](../../../pkg/core/rust-fixtures/core_task_41.json) | 24 | `17ab830f44305a5a06de6d29112a9f12f26cc8ae94d7a492115c36bfdaff1028` |
| [`pkg/core/rust-fixtures/core_task_42.json`](../../../pkg/core/rust-fixtures/core_task_42.json) | 24 | `e96fee79c6e773efd4ed8aeaa832a0fce94ac7425567ff323b743bffe8501c2d` |
| [`pkg/core/rust-fixtures/core_task_43.json`](../../../pkg/core/rust-fixtures/core_task_43.json) | 24 | `f70778d8bd0aebd3d35412c659f9f6c372b64d5bc18b6387e98a4879c351f9c6` |
| [`pkg/core/rust-fixtures/core_task_44.json`](../../../pkg/core/rust-fixtures/core_task_44.json) | 24 | `8f14a423ab1eb25d14c850ca6e096ab83f44a63bd22462909bd7dbdb61ecc42d` |
| [`pkg/core/rust-fixtures/core_task_45.json`](../../../pkg/core/rust-fixtures/core_task_45.json) | 24 | `aac2a626acd041b5bee09c5a4d8229acdabaa62496f1b4c70488746b2e3a14d4` |
| [`pkg/core/rust-fixtures/core_task_46.json`](../../../pkg/core/rust-fixtures/core_task_46.json) | 24 | `6ed9d35f1effda2c32cf13f73418801d83b0aa469d920a4ac8c44c68559cb712` |
| [`pkg/core/rust-fixtures/core_task_47.json`](../../../pkg/core/rust-fixtures/core_task_47.json) | 24 | `0a8d47a1280180653d2d415ef03de7a6d47acef2cb133a16897de9bd1f135d40` |
| [`pkg/core/rust-fixtures/core_task_48.json`](../../../pkg/core/rust-fixtures/core_task_48.json) | 24 | `5b38daa3abd8538c3dcdb53d75a26a6bb14102d9ce60c77fbf58ce9edea23eae` |
| [`pkg/core/rust-fixtures/core_task_49.json`](../../../pkg/core/rust-fixtures/core_task_49.json) | 24 | `9dd25807e3fff31c221c76d89326c346e793b84fffc2ff336a62570e010589b2` |
| [`pkg/core/rust-fixtures/core_task_50.json`](../../../pkg/core/rust-fixtures/core_task_50.json) | 24 | `6d8d7c9e38d1d359a1b62889354779c0a0364676b4cf40e760884c17677745e8` |
| [`pkg/core/rust-fixtures/core_task_51.json`](../../../pkg/core/rust-fixtures/core_task_51.json) | 24 | `1ae2fd97528e0d6760476d3ff96bdcf10a1692fe3ac7b2ccda6091e35d265e29` |
| [`pkg/core/rust-fixtures/core_task_52.json`](../../../pkg/core/rust-fixtures/core_task_52.json) | 24 | `a496003f04bcb2468f47eeb55566cd0c56bdf27405bcd17504bcf3df03ab2bd9` |
| [`pkg/core/rust-fixtures/core_task_53.json`](../../../pkg/core/rust-fixtures/core_task_53.json) | 24 | `22729c329d89ce07d77d5e3a68ec8db81b49f5666c2ca275879fa82dde0bf1c3` |
| [`pkg/core/rust-fixtures/core_task_54-evidence.md`](../../../pkg/core/rust-fixtures/core_task_54-evidence.md) | 24 | `72880a5953dbf02e0ecb2e2628764243e80a0351195aeff019b8377c4a98a3d4` |
| [`pkg/core/rust-fixtures/core_task_54.json`](../../../pkg/core/rust-fixtures/core_task_54.json) | 24 | `e95350e3f2b86857916e7ffae8ffde41031a801c108a9368e3c669b0e875756a` |
| [`pkg/core/rust-fixtures/core_task_55.json`](../../../pkg/core/rust-fixtures/core_task_55.json) | 24 | `f719e598a931f49004ff5d651dc69031d590588a637af9dc43f3400f4328814f` |
| [`pkg/core/rust-fixtures/core_task_56.json`](../../../pkg/core/rust-fixtures/core_task_56.json) | 24 | `b2b22c4c3bda9169a637703e049b9cd63939934811e09bf6cc8ceb720a7c2bfe` |
| [`pkg/core/rust-fixtures/core_task_57.json`](../../../pkg/core/rust-fixtures/core_task_57.json) | 24 | `dde9b8a87056c04a800a2f71e3fc9405709ddb5c998c46d7d2ca64beeba2d728` |
| [`pkg/core/rust-fixtures/core_task_58.json`](../../../pkg/core/rust-fixtures/core_task_58.json) | 24 | `9c07279bdc2a4ec076b58b0582becb4e57542f318089752aea1e1570ed64e541` |
| [`pkg/core/rust-fixtures/core_task_59.json`](../../../pkg/core/rust-fixtures/core_task_59.json) | 24 | `8dea248598e8b0b3f4e495c7f0f9a1ad87688d59ee238c61b26452137b59dc7a` |
| [`pkg/core/rust-fixtures/core_task_60.json`](../../../pkg/core/rust-fixtures/core_task_60.json) | 24 | `72eeb1d864174f0f61ac98c41e357d5965515ce3d5039e5cb5f7b9b21ca2666a` |
| [`pkg/core/rust-fixtures/core_task_61.json`](../../../pkg/core/rust-fixtures/core_task_61.json) | 24 | `7f186f622d782336d8b52cc12f10eb6206898848639cdc8711db06fa26829f38` |
| [`pkg/core/store.go`](../../../pkg/core/store.go) | 24 | `686a3bd66c7deceb3fd79b2c83be359487088993d20e649ba2a533db298ad605` |
| [`pkg/core/store_clone.rs`](../../../pkg/core/store_clone.rs) | 24 | `ee53101080612587b8e9afc1e8563d9371ff18fbe051fd10b995b20948f19be2` |
| [`pkg/core/store_clone_test.rs`](../../../pkg/core/store_clone_test.rs) | 24 | `f4cdce97f690f763a6650985543e7ee0aaa963be9971528e81a8442399666abd` |
| [`pkg/core/store_control.rs`](../../../pkg/core/store_control.rs) | 24 | `b645dad400c514e207fd50232833d8db997c2d11426dbc649409262acbdbaafa` |
| [`pkg/core/store_control_test.rs`](../../../pkg/core/store_control_test.rs) | 24 | `01711f0cfa0a068d01197d3601c3bb264f5e169ed83e9dadb13b3b59868cb4bd` |
| [`pkg/core/store_health.rs`](../../../pkg/core/store_health.rs) | 24 | `3c99ac38bfbb36b863018a84e8a48b9c56554463f2f033f058f47bfa0a7d5c06` |
| [`pkg/core/store_health.rs.md`](../../../pkg/core/store_health.rs.md) | 24 | `9c4e3d88c741562f9020f5a4c16d686094f0f2e3041143bda2958460bd71fa50` |
| [`pkg/core/store_health_test.rs`](../../../pkg/core/store_health_test.rs) | 24 | `55d5c4d887afd3a1e0e6f4b191225d479f05e8bb36b279526cc830c425531faa` |
| [`pkg/core/store_labels.rs`](../../../pkg/core/store_labels.rs) | 24 | `93bf503abdd40cfc75b88477c46324172a1c33f1b0c6e9fabe2848c025b4fe6d` |
| [`pkg/core/store_labels_test.rs`](../../../pkg/core/store_labels_test.rs) | 24 | `26fa9e314b60b68b1c8dbc409f65e49d9bc25c106d3d107cd4f608d187c75b6d` |
| [`pkg/core/store_limit.rs`](../../../pkg/core/store_limit.rs) | 24 | `00c48fc2bb46fce002e666efded5d1f61377efeba4d5487dcd101598472db924` |
| [`pkg/core/store_limit_test.rs`](../../../pkg/core/store_limit_test.rs) | 24 | `529b817208c673ccefb198ee82ef92f5028ca5e02edfbb9d2cf9e0ad25cc4b3f` |
| [`pkg/core/store_location.rs`](../../../pkg/core/store_location.rs) | 24 | `7b34ca578e5b620863c03722defca04f96cee7dd4641307e3a8f9e61df40e92f` |
| [`pkg/core/store_location_test.rs`](../../../pkg/core/store_location_test.rs) | 24 | `b937bf45b383adf394c683cb7049b86224004e55e7b496ecb76a1f03047bca4d` |
| [`pkg/core/store_meta.rs`](../../../pkg/core/store_meta.rs) | 24 | `7eef5f9d706e213a770580d9ab7bb6ad7334f3f9dfc88bbcebe76d647c10ceff` |
| [`pkg/core/store_meta.rs.md`](../../../pkg/core/store_meta.rs.md) | 24 | `fe3512c815d68ba791a8de9301fc43163608a552b2a6c0032a6246a7b67a1ba2` |
| [`pkg/core/store_meta_test.rs`](../../../pkg/core/store_meta_test.rs) | 24 | `a71df26a91648da3129fe416dd71d29a0b133d43d0a4b18892116aea657d0bb3` |
| [`pkg/core/store_option.go`](../../../pkg/core/store_option.go) | 24 | `5f63e5e1153c5461a5a087ee5237344b768c9c65f4d17d3a2105e0f766dc8094` |
| [`pkg/core/store_resource.rs`](../../../pkg/core/store_resource.rs) | 24 | `47f96d5bddff0295c0972a92c3e5fb9599001f92dd50319e7ea01ceacf7de68c` |
| [`pkg/core/store_resource_test.rs`](../../../pkg/core/store_resource_test.rs) | 24 | `9cd9abe1f858bfa35332628dc98d9e95ec9a81fd2d9fa73368d959e62b2f4dc8` |
| [`pkg/core/store_score_v1.rs`](../../../pkg/core/store_score_v1.rs) | 24 | `a8ff0584020d8a2f0c8e2d19ed434f09d9c4133d2c57e7e489ec1f1da24f0803` |
| [`pkg/core/store_score_v1_test.rs`](../../../pkg/core/store_score_v1_test.rs) | 24 | `6a318c0a47743484aee01667c3e3a99054269c087b2332e2f9b0c0486cc8f81d` |
| [`pkg/core/store_score_v2.rs`](../../../pkg/core/store_score_v2.rs) | 24 | `1c92150db371d7371ff8fd2627107f664a02d4ec98f127304a85313c358485c2` |
| [`pkg/core/store_score_v2_test.rs`](../../../pkg/core/store_score_v2_test.rs) | 24 | `567ac5b026d58c2da7c341dc47e55fe6675e6a7af7a970d3ba55550dc08dd4d9` |
| [`pkg/core/store_state.rs`](../../../pkg/core/store_state.rs) | 24 | `54f88a4b7e530dc1c1c64c6f17df68283b61ade7b027c647928285900611774a` |
| [`pkg/core/store_state_test.rs`](../../../pkg/core/store_state_test.rs) | 24 | `ca0f50102441a412adcaa58b5d1a78344f6ec7360aad11d4625af1cd5e3f6fef` |
| [`pkg/core/store_stats.go`](../../../pkg/core/store_stats.go) | 24 | `7eb42ad16235801ee571e52df1fc8aa1d96e226957c1fa479ad6c501ac430246` |
| [`pkg/core/store_stats.rs`](../../../pkg/core/store_stats.rs) | 24 | `a8253b189782ab44c40180e7acd8108b826efb5dc8d1603e24bd331a09252869` |
| [`pkg/core/store_stats.rs.md`](../../../pkg/core/store_stats.rs.md) | 24 | `1e7c1c7f9c0ab139a0d152421764426a255a2328d2851a221677b2643b39e66a` |
| [`pkg/core/store_stats_average.rs`](../../../pkg/core/store_stats_average.rs) | 24 | `fd5a286a46fb76dbbbc121307cb2739be89aa8fd757be3b688ea0235d81734b6` |
| [`pkg/core/store_stats_average_test.rs`](../../../pkg/core/store_stats_average_test.rs) | 24 | `af8d65b80bee80982f34430eb175917698728ba8f5aa72ac351f37164a1a2cb9` |
| [`pkg/core/store_stats_test.go`](../../../pkg/core/store_stats_test.go) | 24 | `bee43bbecaca4c49f8b11744f8f9aaa8ede3d811a6626d65b68c23bbe699da9d` |
| [`pkg/core/store_stats_test.rs`](../../../pkg/core/store_stats_test.rs) | 24 | `29c15ad2d2ff9a1958bfd1c07a137180cf84109475066b2ee9f3db4a19c0937d` |
| [`pkg/core/store_test.go`](../../../pkg/core/store_test.go) | 24 | `ee6b06adc08545498c285b357654f125c8c4e942548ae6110a6e36b0756334f0` |
| [`pkg/core/store_time.rs`](../../../pkg/core/store_time.rs) | 24 | `7707a12301f21b9ac0ef78b00cb38f438c0ecef918305c6b2ca387925722aa8d` |
| [`pkg/core/store_time_test.rs`](../../../pkg/core/store_time_test.rs) | 24 | `579fd10e33d33bd781e3b98d30419bb47d622c42c7677d832dd88d5c837672f8` |
| [`pkg/core/store_topology.go`](../../../pkg/core/store_topology.go) | 24 | `be0df903be3f2506478f9c7df03109312b780ac34fdbed46c97eb9ac3b3c7710` |
| [`pkg/core/store_topology.rs`](../../../pkg/core/store_topology.rs) | 24 | `99b63047f702962b90bcd95bc739680b9e3888a4c22448a37ecda706f1a32901` |
| [`pkg/core/store_topology_test.go`](../../../pkg/core/store_topology_test.go) | 24 | `cdfbbde01a1fc91fb3e8cab9641d9fc33aa3dec43c4c558d72e865c45bb46136` |
| [`pkg/core/store_topology_test.rs`](../../../pkg/core/store_topology_test.rs) | 24 | `addcb87e4378e148d757a0867bdf5c7834ed0bc75e4fdfbbe609b694268b3bae` |
| [`pkg/core/storelimit/Cargo.toml`](../../../pkg/core/storelimit/Cargo.toml) | 24,54 | `922ba0f405972ebcae5e62c4e09aa1178fe0c93047c4fd80aaa8382d57d6fbf2` |
| [`pkg/core/storelimit/lib.rs`](../../../pkg/core/storelimit/lib.rs) | 24,54 | `96bf6fdcfb29999347efe084df3d99f203985d61057c96a1f2484631a0a8f483` |
| [`pkg/core/storelimit/limit.go`](../../../pkg/core/storelimit/limit.go) | 24,54 | `83a78f086ffe4bd220338f9b37452441538ce5841d903ea67ca2dafc0991fb4c` |
| [`pkg/core/storelimit/limit_test.go`](../../../pkg/core/storelimit/limit_test.go) | 24,54 | `ed75efb2691bcbcf49f3fb13a1c7d41116fc4de41b3a84e116f6a12fd5d3b872` |
| [`pkg/core/storelimit/limit_types.rs`](../../../pkg/core/storelimit/limit_types.rs) | 24,54 | `9a159d9f2b50402d2d881942403ca1f0f2dff542fcec78256f835c1094f992e0` |
| [`pkg/core/storelimit/limit_types_test.rs`](../../../pkg/core/storelimit/limit_types_test.rs) | 24,54 | `18be58bffcd22196f443efd88a73844c8d43fc6bf6c7e3103e6d82e387d9e13c` |
| [`pkg/core/storelimit/rate_limit.rs`](../../../pkg/core/storelimit/rate_limit.rs) | 24,54 | `614febef21f93744085fa305ec2dd9aac9be2182409505365545824468e44ec5` |
| [`pkg/core/storelimit/rate_limit_test.rs`](../../../pkg/core/storelimit/rate_limit_test.rs) | 24,54 | `60007fc860f1106d5128540fc2ea5b497c2c51bd77ac40073ed44f158b415ae7` |
| [`pkg/core/storelimit/rate_reset.rs`](../../../pkg/core/storelimit/rate_reset.rs) | 24,54 | `138f809824c3347b9bf5ecd4438f2b5545625d79cfc77a3fa095fdf0d1569a09` |
| [`pkg/core/storelimit/rate_reset_test.rs`](../../../pkg/core/storelimit/rate_reset_test.rs) | 24,54 | `246d1706e556236071dd79277c66bb607cec2c4a459be503f9fa574ae6c2a1cc` |
| [`pkg/core/storelimit/rust-fixtures/core_task_02.json`](../../../pkg/core/storelimit/rust-fixtures/core_task_02.json) | 24,54 | `1b2659bffa4adea8faeceb3b748c1fe7a944bd10f9330becc4607941c8c6662e` |
| [`pkg/core/storelimit/rust-fixtures/core_task_03.json`](../../../pkg/core/storelimit/rust-fixtures/core_task_03.json) | 24,54 | `5e9d558329413e207afb34ea44a8189dc8bd97b5b0612a9056f27cb800bfa38d` |
| [`pkg/core/storelimit/rust-fixtures/core_task_04.json`](../../../pkg/core/storelimit/rust-fixtures/core_task_04.json) | 24,54 | `17729704fd037d582c22e18ca3029d13078dfea570dbfb87e98447710b43d176` |
| [`pkg/core/storelimit/rust-fixtures/core_task_05.json`](../../../pkg/core/storelimit/rust-fixtures/core_task_05.json) | 24,54 | `e6c4bbcf5aa9b01b9cea9674eef7ddec9348d7eb476c83dcf0790e64bc431d2e` |
| [`pkg/core/storelimit/rust-fixtures/core_task_06.json`](../../../pkg/core/storelimit/rust-fixtures/core_task_06.json) | 24,54 | `6c4a75ac3d1c24773534e098e89e5d73e4014fff7d1390f5f7c00c1a3fa793c0` |
| [`pkg/core/storelimit/rust-fixtures/core_task_07.json`](../../../pkg/core/storelimit/rust-fixtures/core_task_07.json) | 24,54 | `aebc64d3e1340d63c8ea14f15030a2b02acdb9830eef558e52ad5c0130141e82` |
| [`pkg/core/storelimit/sliding_window.go`](../../../pkg/core/storelimit/sliding_window.go) | 24,54 | `3562c93bca2c168971110f2cfc1cc35df0ab432b2b1ec31ca6a82b3abec535ac` |
| [`pkg/core/storelimit/sliding_windows.rs`](../../../pkg/core/storelimit/sliding_windows.rs) | 24,54 | `c40984c13175c1294392fa6fcda4e20f8c223d28273162976c38182d3ee9eaf8` |
| [`pkg/core/storelimit/sliding_windows_test.rs`](../../../pkg/core/storelimit/sliding_windows_test.rs) | 24,54 | `6e5971e257d5636c6302ec69ef69ad3d2ec4e10e2c0cb963114e071cdae826f3` |
| [`pkg/core/storelimit/store_limit.go`](../../../pkg/core/storelimit/store_limit.go) | 24,54 | `1556e3bee72d2d5e218a425938c43505385a2c6af5c91cd3d26bde9ccd507651` |
| [`pkg/core/storelimit/window.rs`](../../../pkg/core/storelimit/window.rs) | 24,54 | `48b0f335dd2d529fbec392526756e937682080fc80ad81c6da089403234882e4` |
| [`pkg/core/storelimit/window_feedback.rs`](../../../pkg/core/storelimit/window_feedback.rs) | 24,54 | `0daa82ac2c385733a340e9cd094a357f93a32891954c81666b3c6fe3964f6e9d` |
| [`pkg/core/storelimit/window_feedback_test.rs`](../../../pkg/core/storelimit/window_feedback_test.rs) | 24,54 | `54e1603f5099807d495099f6dec277416a6c9ac370871d6981fa52eb77c798e0` |
| [`pkg/core/storelimit/window_test.rs`](../../../pkg/core/storelimit/window_test.rs) | 24,54 | `24f7e972f98b1c7bba3ecb333e13d485243086ba9262e996f404f66fc52af2a4` |
| [`pkg/core/stores.rs`](../../../pkg/core/stores.rs) | 24 | `b3f8ad8f900ef5cfe59883e0d296ba3a47002fe3a3abd3f438469f4cc95cd045` |
| [`pkg/core/stores_control.rs`](../../../pkg/core/stores_control.rs) | 24 | `8985f117d43c1b40c3518e973f79a2b2ffa852db8ff0e957f2b2ce78c4b4c42a` |
| [`pkg/core/stores_control_test.rs`](../../../pkg/core/stores_control_test.rs) | 24 | `4429dcf640c6953de6b65e558fa13fb796518c5bb6702a41fec154eb6309feb1` |
| [`pkg/core/stores_region.rs`](../../../pkg/core/stores_region.rs) | 24 | `ad137779953cfa73e471008ac3ac05e72ff3f92fbecb1296fa2de355684c296f` |
| [`pkg/core/stores_region_test.rs`](../../../pkg/core/stores_region_test.rs) | 24 | `403c73ee6695a611d91ad0c1d9bca302520b191f07dd2960e23f544d4ef0d6b9` |
| [`pkg/core/stores_test.rs`](../../../pkg/core/stores_test.rs) | 24 | `c7fb714fbf4086858c64e4c6e915803523bca97d2edb193426b76cda7ffd7b0e` |
| [`pkg/id/Cargo.toml`](../../../pkg/id/Cargo.toml) | 21,26 | `244c8055fb22a06a94278eac72e0c0d1b6cd3a688f1444703ccce8d6a769776a` |
| [`pkg/id/allocate_count_test.rs`](../../../pkg/id/allocate_count_test.rs) | 21,26 | `c85f15dae26c52b26133c84f706f3a3bf8750b5176ff7158ac442b63ca1339d8` |
| [`pkg/id/allocator.rs`](../../../pkg/id/allocator.rs) | 21,26 | `04d7112ba608531f3cdf47d837b739c7781ffd5527731fb267ddf9e86bf90f6a` |
| [`pkg/id/allocator_base_test.rs`](../../../pkg/id/allocator_base_test.rs) | 21,26 | `916f9cc7e741b65b29500f41a676379ca03bde2ab82152370f39d7fcf674cc69` |
| [`pkg/id/allocator_namespace.rs`](../../../pkg/id/allocator_namespace.rs) | 21,26 | `6dc89a490b22cae760d1e5bdb0094b8dd0ac6ab97dcb4c206bf81bcc4da02237` |
| [`pkg/id/allocator_namespace_test.rs`](../../../pkg/id/allocator_namespace_test.rs) | 21,26 | `316ec66724d3f030b6c1c9fb0e0eb1c7d562fc291a330e518d9f2d1f615a7f5a` |
| [`pkg/id/allocator_test.rs`](../../../pkg/id/allocator_test.rs) | 21,26 | `e89e5dd14de594ee51b75c378cff5aa3863c982ba42feb6c36ac53e2b2e647de` |
| [`pkg/id/etcd_behavior_test.rs`](../../../pkg/id/etcd_behavior_test.rs) | 21,26 | `b28e9ac1cbd915b7fc2e5e373ae58a71eedfc9bcbf3a4795886844ae206230c1` |
| [`pkg/id/id.go`](../../../pkg/id/id.go) | 21,26 | `ce6835b3aba6b281e44197ef36b17a20c9e2e7857d30d4cd999e26a15cd01c84` |
| [`pkg/id/id_test.go`](../../../pkg/id/id_test.go) | 21,26 | `1fe8e34a2e5dfce53e9ce008754a4b9c32d590992fc7d105eccf9d70edf7a9ed` |
| [`pkg/id/lib.rs`](../../../pkg/id/lib.rs) | 21,26 | `8c25c45d5563363adb5fe7815014f64d3e48882f8ad12fa4afdc4b0441d1bb18` |
| [`pkg/id/metrics.go`](../../../pkg/id/metrics.go) | 21,26 | `613224400397f0de556f37895880861d610afd605507f2403dbd34fa10d8065b` |
| [`pkg/member/Cargo.toml`](../../../pkg/member/Cargo.toml) | 7,44,45,89,98 | `6e82fc4741bbbe4c503295f8db7bb268bc23d9f6940d47259c11d5db4ba6db7d` |
| [`pkg/member/election.go`](../../../pkg/member/election.go) | 7,44,45,89,98 | `6ef9e0ceb5bdaef53809e7aad5d65c88f62f7b08453d1b6dc3da1e4c2eb32731` |
| [`pkg/member/etcd_behavior_test.rs`](../../../pkg/member/etcd_behavior_test.rs) | 7,44,45,89,98 | `581ac1d2298a6144241ea8ec786be6a5731b53a68f2db725a15c90424451c379` |
| [`pkg/member/leader_priority.rs`](../../../pkg/member/leader_priority.rs) | 7,44,45,89,98 | `e0f490b3c25444fb0cad2e6cb1a6c0e985a422a003e2bfb16bd92e48883628fe` |
| [`pkg/member/leader_priority_test.rs`](../../../pkg/member/leader_priority_test.rs) | 7,44,45,89,98 | `57ccb8260ac347a7102686e882c70867835539c8764ca16cb1762dbeee81c533` |
| [`pkg/member/lib.rs`](../../../pkg/member/lib.rs) | 7,44,45,89,98 | `448ca6c3fd0feb9121252042d7cf986b677453ad87cf17a4ac16c4ae8fae1bf8` |
| [`pkg/member/member.go`](../../../pkg/member/member.go) | 7,44,45,89,98 | `2e637d2a57d70bdc37db846fff5c3ca824af1741000a6b1925b56c6351f8c102` |
| [`pkg/member/member.rs`](../../../pkg/member/member.rs) | 7,44,45,89,98 | `523fcf71a050e9e64e3e62dd6ad9dca5d5981bb8c601fff126100cac88375903` |
| [`pkg/member/member_info.rs`](../../../pkg/member/member_info.rs) | 7,19,44,45,89,98 | `223098c2ff4b8e334f668856d81a66a1786ebec116a68d4d306dc895adee2dc2` |
| [`pkg/member/member_info_test.rs`](../../../pkg/member/member_info_test.rs) | 7,44,45,89,98 | `38012a3a5584287eac43981933c7ad893ac181599cf11ace1b3867eede020f90` |
| [`pkg/member/member_test.rs`](../../../pkg/member/member_test.rs) | 7,44,45,89,98 | `c214f099f9cb8623ebbac6904b3bc4a603777cf42e85fdb0b534931f38784df8` |
| [`pkg/member/metrics.go`](../../../pkg/member/metrics.go) | 7,44,45,89,98 | `2b21d972aa67b340ecd0809f8eebc2c4b211e7b0cfd6363e0112e7ee7b4539c6` |
| [`pkg/member/participant.go`](../../../pkg/member/participant.go) | 7,44,45,89,98 | `5cc0d4118c3fde9c7f266749ff2c16a0e0a0c7d44c9b278401a2562ed4f4a776` |
| [`pkg/progress/Cargo.toml`](../../../pkg/progress/Cargo.toml) | 54 | `71b04b818e67fc4bb1368c9a12145bbfd0e55d0be6b078da8a6cea88de213423` |
| [`pkg/progress/indicator.go`](../../../pkg/progress/indicator.go) | 54 | `bd093cc179fd5edfd81f2289f8d8a3e0417221834c5a0c1cbcac221f17224727` |
| [`pkg/progress/indicator.rs`](../../../pkg/progress/indicator.rs) | 54 | `9184bbefe66c27e37e26f2f7ba994713984c2a7e8495e721bdcba879d6bbe5d4` |
| [`pkg/progress/indicator.rs.md`](../../../pkg/progress/indicator.rs.md) | 54 | `e45f6134e169a57ac0161a0e5ce2676322eb7d4d67901797b5a1d53329410426` |
| [`pkg/progress/indicator_test.go`](../../../pkg/progress/indicator_test.go) | 54 | `f0dd8dc754ac4988c8900745efb30b1f991dff647a6f7ea6fa22c98b57249fa2` |
| [`pkg/progress/lib.rs`](../../../pkg/progress/lib.rs) | 54 | `1b52ea0b8a320eaa018353652f73f65128d3449b9044a482079de3fa201e9c81` |
| [`pkg/progress/lib.rs.md`](../../../pkg/progress/lib.rs.md) | 54 | `819c3e505a268e359d1afa3ff1a541bbb6a5a3ead74cf54ccbcf1ff0567f2c0f` |
| [`pkg/progress/metrics.go`](../../../pkg/progress/metrics.go) | 54 | `64639f41a9bfe3b634821604c2647d88c30b9f0b73f364d5a17e2da7eac3df81` |
| [`pkg/progress/progress.go`](../../../pkg/progress/progress.go) | 54 | `57c3b472bdf1fb567d9189f7da401e5d415ed31ed1b2ca080afdbda50e75864e` |
| [`pkg/progress/progress_manager.rs`](../../../pkg/progress/progress_manager.rs) | 54 | `df45af877b3138ac381f74a4e6d65e16750612f0ad89e5e019ff1b2dc1b03f28` |
| [`pkg/progress/progress_manager.rs.md`](../../../pkg/progress/progress_manager.rs.md) | 54 | `6302cf43022bd0426032f8c0c5316acff7ec1f7b6f5742d096b3255a9a51aaab` |
| [`pkg/progress/progress_test.go`](../../../pkg/progress/progress_test.go) | 54 | `a78324c856d7abe1ed1fb0b12a26e2a51bacc9a95eaae6093d7218133de0a4a2` |
| [`pkg/progress/task_130_test.rs`](../../../pkg/progress/task_130_test.rs) | 54 | `0cfef26eb0ea9357d7432c2de372d6f1f49b7c330790cd2d05b66ea7d3f009cd` |
| [`pkg/progress/task_131_test.rs`](../../../pkg/progress/task_131_test.rs) | 54 | `223f94b4a823fcb5bae0b3e3ca4d86ca877fb3ab5c208219db5372633a60b055` |
| [`pkg/progress/task_132_test.rs`](../../../pkg/progress/task_132_test.rs) | 54 | `88fabf749d196ffb5a6708370e7914969315826eb13f081e64f41a5caa4947e2` |
| [`pkg/proto/Cargo.toml`](../../../pkg/proto/Cargo.toml) | 102 | `b89bd94f679f113f75b2938e46cbb6a17158baedd9088db4a9eb90a793391407` |
| [`pkg/proto/build.rs`](../../../pkg/proto/build.rs) | 2,4 | `338bd840931db1138d92256806f81430b68761e6e972cd7721cfcb3c179033f1` |
| [`pkg/proto/build.rs.md`](../../../pkg/proto/build.rs.md) | 102 | `38c4b7ae254d873b45e6cb4fd240f6283e8f16e55945ebacc167b6191dc7a2f6` |
| [`pkg/proto/lib.rs`](../../../pkg/proto/lib.rs) | 2,37,100 | `ea109feac2ae247228e9026249d4cfed1dbba407e8857d7e4788dfe331f78ed5` |
| [`pkg/proto/proto_compat_test.rs`](../../../pkg/proto/proto_compat_test.rs) | 102 | `cc0115773848bf4144d13b3a6455f70edd45def0283f93a709dea393d40b5aa6` |
| [`pkg/proto/wire-fixtures.json`](../../../pkg/proto/wire-fixtures.json) | 102 | `7116298b15785761da3ebbcfe84e71059d73768faf4a146aaf256ae29580d632` |
| [`pkg/ratelimit/Cargo.toml`](../../../pkg/ratelimit/Cargo.toml) | 79,80 | `5e50bdc7f0467773f56a6b7c0390defc8923f7cbe0ba7afaf15f12bbdbab59db` |
| [`pkg/ratelimit/concurrency.rs`](../../../pkg/ratelimit/concurrency.rs) | 79,80 | `22ad205a3dbc6d7726337363758e65260e3aeecb6c80d0bace4786b7620362b2` |
| [`pkg/ratelimit/concurrency_limiter.go`](../../../pkg/ratelimit/concurrency_limiter.go) | 79,80 | `0f9400a3d1d32611e57ce39d808998be7e47e6e4846c536c69706a2497490849` |
| [`pkg/ratelimit/concurrency_limiter_test.go`](../../../pkg/ratelimit/concurrency_limiter_test.go) | 79,80 | `1f9dcb9ec538255859e36b6f39d18ec5bbcba4c4efe317ff35a243fe1601467e` |
| [`pkg/ratelimit/concurrency_test.rs`](../../../pkg/ratelimit/concurrency_test.rs) | 79,80 | `d4d5d6bef8124ec71af01692c4ba99156ce379bafe91761cd03482f0e7e7c5e0` |
| [`pkg/ratelimit/controller.go`](../../../pkg/ratelimit/controller.go) | 79,80 | `13fcc7094d67fa0452a5d66d6a265ba73daf69668cbf51f56a5d6614e2c04856` |
| [`pkg/ratelimit/controller.rs`](../../../pkg/ratelimit/controller.rs) | 79,80 | `d13e6e1884bb615682960189607a7f64717a7008a0fb879e181062b82c5e88ed` |
| [`pkg/ratelimit/controller_test.go`](../../../pkg/ratelimit/controller_test.go) | 79,80 | `1bc671df5fb5891a98d03a3a14939174cf766e5dd8b5b03b4a03140e801b0a41` |
| [`pkg/ratelimit/controller_test.rs`](../../../pkg/ratelimit/controller_test.rs) | 79,80 | `b59d4639abc5384dfb87dd91058ca5a2ab358d1ca37d429ce737a877c6832cab` |
| [`pkg/ratelimit/lib.rs`](../../../pkg/ratelimit/lib.rs) | 79,80 | `8bb83edea325afe0c73682ff7965c0d16aba361d47c284601561bc53ef03dade` |
| [`pkg/ratelimit/limiter.go`](../../../pkg/ratelimit/limiter.go) | 79,80 | `00f776a44c9574caedf725aed04974c7911f26a31d1173055397ef2ce26cadab` |
| [`pkg/ratelimit/limiter.rs`](../../../pkg/ratelimit/limiter.rs) | 79,80 | `0fdf255f12be9c360fdd367aa72ef93e15a90bd717007b969bd510333f4e68e8` |
| [`pkg/ratelimit/limiter_test.go`](../../../pkg/ratelimit/limiter_test.go) | 79,80 | `6e07a8571411adcbb095771b59b850655fecf13836237bac80940474793bc267` |
| [`pkg/ratelimit/limiter_test.rs`](../../../pkg/ratelimit/limiter_test.rs) | 79,80 | `a731eff4ba47eb494f905de02344566126a31dfb699b20b8bee721310077155d` |
| [`pkg/ratelimit/metrics.go`](../../../pkg/ratelimit/metrics.go) | 79,80 | `b640bd11663bd0945f22d9ab1388129ef9e37ae88c9b4a75c250ad408df3d7f9` |
| [`pkg/ratelimit/option.go`](../../../pkg/ratelimit/option.go) | 79,80 | `ed62c63a616185d0a90e3424471cdc49828ec094822203090bf685ca3392656d` |
| [`pkg/ratelimit/rate_limiter.rs`](../../../pkg/ratelimit/rate_limiter.rs) | 79,80 | `6e757938d1730e4892691c7fbdb9c3ee6af008dc99b5d546d3a88bec2ce7d21e` |
| [`pkg/ratelimit/rate_limiter_test.rs`](../../../pkg/ratelimit/rate_limiter_test.rs) | 79,80 | `4d7e88ed045eb43e9a6729e105df3cd56a07dea36eb9303cb9e503d8f96b58f6` |
| [`pkg/ratelimit/ratelimiter.go`](../../../pkg/ratelimit/ratelimiter.go) | 79,80 | `0ae575cc095e8791b3be7e0b2e5e74716253242465ba28d556eaa388dc32a877` |
| [`pkg/ratelimit/ratelimiter_test.go`](../../../pkg/ratelimit/ratelimiter_test.go) | 79,80 | `25d6996f724aef9be8d3273432aafa735b02ac0fd2447e1132d41a4d56413dc5` |
| [`pkg/ratelimit/runner.go`](../../../pkg/ratelimit/runner.go) | 79,80 | `ef4a8a1c6d84228bfb9432bba5a5f1e754f78af15c40219d4c1ed5e5c9bcf190` |
| [`pkg/ratelimit/runner.rs`](../../../pkg/ratelimit/runner.rs) | 79,80 | `65b440b5fd9917c5485191f95f1c2e7c3f3bbb95b99f7c2f957f480ebdfdff68` |
| [`pkg/ratelimit/runner_benchmark_test.go`](../../../pkg/ratelimit/runner_benchmark_test.go) | 79,80 | `daf04ed231dea218addf7b900b4a279716cc142188053f55883b18d47dc7ffe3` |
| [`pkg/ratelimit/runner_test.go`](../../../pkg/ratelimit/runner_test.go) | 79,80 | `6f54a1027fcea9d4292467c6c76d0bdb7f4d48e5cb7578d8f129c0b0f5452102` |
| [`pkg/ratelimit/runner_test.rs`](../../../pkg/ratelimit/runner_test.rs) | 79,80 | `3ab35c3501689471f9d95e122d5e9c867523ed203da1213f6c413d53a78ad50e` |
| [`pkg/response/Cargo.toml`](../../../pkg/response/Cargo.toml) | 55,56 | `6662ce859df2e76f62c55d6df95b7b3db1d637da8efe935047b74cc1f1625a31` |
| [`pkg/response/lib.rs`](../../../pkg/response/lib.rs) | 55,56 | `e699ac3c1e159a656883343348b6f4abb247ffe7727e78588f6f36bd092f455d` |
| [`pkg/response/peer.rs`](../../../pkg/response/peer.rs) | 55,56 | `9db02d6b48d171ae66aab925399b07160322b4ddc33ccb339c347cbdf3f06c01` |
| [`pkg/response/peer_test.rs`](../../../pkg/response/peer_test.rs) | 55,56 | `7affec1aab1418ae208c1aeb54a1db10ca4f18ad2962c4e3a9e4c17d1f7f2ba7` |
| [`pkg/response/region.go`](../../../pkg/response/region.go) | 55,56 | `ce403a9ecd74f7d7bdf17fd9a31acc721bc9d854cf7dff9c91f9d0c4cc47e30f` |
| [`pkg/response/region.rs`](../../../pkg/response/region.rs) | 55,56 | `5142f427aa648b8b665bf2f85321da257e75d8ab248ec671eeee8c80ed3354a4` |
| [`pkg/response/region.rs.md`](../../../pkg/response/region.rs.md) | 55,56 | `cf0141aef6614b1768cc63659b220e70305ae8efc7bdd03e3b30cc4f8d102223` |
| [`pkg/response/region_easyjson.go`](../../../pkg/response/region_easyjson.go) | 55,56 | `4431f4e7120be79e2d1c78e271496fa65b00e3132813b513be993c4a0a66b9f7` |
| [`pkg/response/region_test.go`](../../../pkg/response/region_test.go) | 55,56 | `43e36b1b24169e16c7cec8ea91c2f2e8026c70dd2aa5c50427a40cc05fa2639c` |
| [`pkg/response/region_test.rs`](../../../pkg/response/region_test.rs) | 55,56 | `eafaf98364d501d3e55c8cfc34702c8a816279c28589050f9054a3e17babb5d5` |
| [`pkg/response/regions.rs`](../../../pkg/response/regions.rs) | 55,56 | `62db3361068e75284af2d4b9e3b799b1a5470a2c831fe46384586c92ff1d252d` |
| [`pkg/response/regions.rs.md`](../../../pkg/response/regions.rs.md) | 55,56 | `a21644299846349b7b3e37dd0d0042cff49f0f8b026aac07b8bd9d029a6de756` |
| [`pkg/response/regions_test.rs`](../../../pkg/response/regions_test.rs) | 55,56 | `e43a03507a867b9d2e18fb252785bd5986e8c36e7d5462985df62b9e4e7f0f3b` |
| [`pkg/response/rust-fixtures/core_task_63.json`](../../../pkg/response/rust-fixtures/core_task_63.json) | 55,56 | `4654981a194de83fe2deb9e353a5fd2be199baa105d005fdb6edcde60754d3d9` |
| [`pkg/response/rust-fixtures/core_task_64.json`](../../../pkg/response/rust-fixtures/core_task_64.json) | 55,56 | `84aa495385febab98f875b3d5f2618172287214bf80a56ce3417e5a1b685e746` |
| [`pkg/response/rust-fixtures/core_task_65.json`](../../../pkg/response/rust-fixtures/core_task_65.json) | 55,56 | `d47393bba0340ae44c77a75cd3726d5cdfb14fddd0ea345a2641237548199f10` |
| [`pkg/response/rust-fixtures/core_task_66.json`](../../../pkg/response/rust-fixtures/core_task_66.json) | 55,56 | `029dc8d1c848af291550233663ac1148988a120d4a40f21b6f72af7cf3307eb0` |
| [`pkg/response/rust-fixtures/core_task_67.json`](../../../pkg/response/rust-fixtures/core_task_67.json) | 55,56 | `1d246436a505bd279a3ae5c55fc17330f588bedf7e9ecaf95868a970da9eca54` |
| [`pkg/response/store.go`](../../../pkg/response/store.go) | 55,56 | `b5764461e51092d0b9d9057721cbd6049da9091837ecc4034646054b0b830e77` |
| [`pkg/response/store.rs`](../../../pkg/response/store.rs) | 55,56 | `f63e33769262e9f50361a906544aa92e95028a9b71dad7b7170bfbf3e32e5e5e` |
| [`pkg/response/store.rs.md`](../../../pkg/response/store.rs.md) | 55,56 | `99749386deefcce5ca7efe421fd390c9b5c0a775eebbd3a7bedf3a8b0d998128` |
| [`pkg/response/store_state.rs`](../../../pkg/response/store_state.rs) | 55,56 | `9e1d3304e12b678fb00ccb5e73996e4bc00045f99766564ee95724132f554e0d` |
| [`pkg/response/store_state.rs.md`](../../../pkg/response/store_state.rs.md) | 55,56 | `1a0517f0df2591d13844567b88ba0a99cbd61f40273c5fbe0bf40dcd3e7ba6e3` |
| [`pkg/response/store_state_test.rs`](../../../pkg/response/store_state_test.rs) | 55,56 | `5e77d40e801c0b576befd3496887cac65ce3b1d4e2d4b0dbd4ddc25c73051971` |
| [`pkg/response/store_test.rs`](../../../pkg/response/store_test.rs) | 55,56 | `e8cc80931e33ce259d829b0405b432b2bdf4626a794f6e01e32327ddef8366b5` |
| [`pkg/runtime/Cargo.toml`](../../../pkg/runtime/Cargo.toml) | 3,85 | `0c1654eadd5b2d0be4dff887b0a31b7497d90420e74edda54f8992954c7c3f4f` |
| [`pkg/runtime/lib.rs`](../../../pkg/runtime/lib.rs) | 3,85 | `6f7c75b008cb1b11a00ab2cf10be1ba196e11e463a3f4f84d5a44a5600f136e9` |
| [`pkg/runtime/node.rs`](../../../pkg/runtime/node.rs) | 1,3,5,7,83,84,85,86,87,89,93,94,95,98 | `aaaf21d8c9010cecae983bf7ef44015a3a624856bf67874003c0fa94b772774c` |
| [`pkg/runtime/node_test.rs`](../../../pkg/runtime/node_test.rs) | 3,85,96 | `98cfe2e0830816367ea9c85adfa38f07b940963c87069274ad1d77925d820513` |
| [`pkg/schedule/Cargo.toml`](../../../pkg/schedule/Cargo.toml) | 95 | `01a879ccf620d43f63a30bf62e3d3dd50ad0e254e708a51cc417bb3b86dfe743` |
| [`pkg/schedule/affinity/Cargo.toml`](../../../pkg/schedule/affinity/Cargo.toml) | 71,72,95 | `12c82bc52372b4e4e2089650475123d387a8143fc932ad8fd7bb1179410cdf94` |
| [`pkg/schedule/affinity/affinity_availability_test.rs`](../../../pkg/schedule/affinity/affinity_availability_test.rs) | 71,72,95 | `15eb5806ed66822f693c33f740d3eff58fe090bbee1fbfba0cc082ede1ad5fd4` |
| [`pkg/schedule/affinity/affinity_cache_sync_test.rs`](../../../pkg/schedule/affinity/affinity_cache_sync_test.rs) | 71,72,95 | `ffb3f584bbec3f8bcda03573d9765d90ffefa09b20ad396763d87f4864b84351` |
| [`pkg/schedule/affinity/affinity_create_delete_test.rs`](../../../pkg/schedule/affinity/affinity_create_delete_test.rs) | 71,72,95 | `09a7d8057bb4e80d17f8e19a049229483f336387a059b3e648f78c67d7c49401` |
| [`pkg/schedule/affinity/affinity_lifecycle_test.rs`](../../../pkg/schedule/affinity/affinity_lifecycle_test.rs) | 71,72,95 | `15006087055964306a36bc0582eec5cb091448c072ff108e78bb83f03b4c2602` |
| [`pkg/schedule/affinity/affinity_ranges_test.rs`](../../../pkg/schedule/affinity/affinity_ranges_test.rs) | 71,72,95 | `1e27b2238d1a05690d0be4938a4ad9c7022975f624ae3b24dee7843c52dd9a65` |
| [`pkg/schedule/affinity/affinity_restore_test.rs`](../../../pkg/schedule/affinity/affinity_restore_test.rs) | 71,72,95 | `f71a9c1dd4e49531fa600a5b37ffa417e9cede854cf412d450501ddea2f85153` |
| [`pkg/schedule/affinity/affinity_split_test.rs`](../../../pkg/schedule/affinity/affinity_split_test.rs) | 71,72,95 | `c0fa404af276eac904060982e3909f270a80130dba3526eaa22d079fbb841461` |
| [`pkg/schedule/affinity/affinity_test.go`](../../../pkg/schedule/affinity/affinity_test.go) | 71,72,95 | `f25737a02a3671d54212fe3457596babe144c16ab177e0b0637e4b4f97656563` |
| [`pkg/schedule/affinity/affinity_update_peers_test.rs`](../../../pkg/schedule/affinity/affinity_update_peers_test.rs) | 71,72,95 | `e72def4d5a50f17f2c67a4b9d6a52b3edb8650718a27079f1b04ac6f484813c4` |
| [`pkg/schedule/affinity/affinity_update_ranges_test.rs`](../../../pkg/schedule/affinity/affinity_update_ranges_test.rs) | 71,72,95 | `b69f4aed18223c9d50206039b18ed657d9c0d154da93cda7d2a2f022bb976dd0` |
| [`pkg/schedule/affinity/affinity_values_test.rs`](../../../pkg/schedule/affinity/affinity_values_test.rs) | 71,72,95 | `7f24445dfd9dd403db1ff056eee3c9e7f503c01a78dd1ed247d157323d01b646` |
| [`pkg/schedule/affinity/availability_loop.rs`](../../../pkg/schedule/affinity/availability_loop.rs) | 71,72,95 | `1e6ea2c1e19beaa74e2e2fded31660e5ca108dafea0707e290a5deacce2d4e58` |
| [`pkg/schedule/affinity/availability_loop.rs.md`](../../../pkg/schedule/affinity/availability_loop.rs.md) | 71,72,95 | `28f46789f1fcffcbbcc2bfd2dff8475b0e181aba09b71634ed44395aec4a9ef7` |
| [`pkg/schedule/affinity/group.go`](../../../pkg/schedule/affinity/group.go) | 71,72,95 | `e87119012a158c316534e2e015c7fb4a9f3387823bf67eb20fee9065ef06f012` |
| [`pkg/schedule/affinity/group.rs`](../../../pkg/schedule/affinity/group.rs) | 71,72,95 | `e9ed70240fb2a3d8b1537340d7db5f848dbab6025df63ecc975f70e89d7cf86e` |
| [`pkg/schedule/affinity/group.rs.md`](../../../pkg/schedule/affinity/group.rs.md) | 71,72,95 | `e572cfc05d5c8acdf9351aaef48054e25ea10ece89187f62d4535b7793d16066` |
| [`pkg/schedule/affinity/group_test.go`](../../../pkg/schedule/affinity/group_test.go) | 71,72,95 | `2673267ce4477431add8fbbdd146c63568af6eff152240a803fdd7916ea00791` |
| [`pkg/schedule/affinity/lib.rs`](../../../pkg/schedule/affinity/lib.rs) | 71,72,95 | `5f9125ff1dfd133aacca0827772f003e97fc746d3017c057a3a86947ccc3b6c3` |
| [`pkg/schedule/affinity/manager.go`](../../../pkg/schedule/affinity/manager.go) | 71,72,95 | `65c9eed5ad88ab7eea50554317967f25c7161d86eff365451cf2d96fb281c31b` |
| [`pkg/schedule/affinity/manager.rs`](../../../pkg/schedule/affinity/manager.rs) | 71,72,95 | `0d36df9d8385100d3bdb4861dc74e54e4c3f80a821d146387307f11917b4f2ac` |
| [`pkg/schedule/affinity/manager.rs.md`](../../../pkg/schedule/affinity/manager.rs.md) | 71,72,95 | `4ef6f3eef8bcc5f10b9641019527ed9aeb82c901d6f9f6b554b50b873c1db3d5` |
| [`pkg/schedule/affinity/manager_test.go`](../../../pkg/schedule/affinity/manager_test.go) | 71,72,95 | `536d5d2455b44ffa30bd67f79eaf964cc6df7581c5bf631ec2bbf9d9d1b93257` |
| [`pkg/schedule/affinity/metrics.go`](../../../pkg/schedule/affinity/metrics.go) | 71,72,95 | `3adf1df35e7841e489b313d55d919ea83c36127f37164229cde9be1781898f1b` |
| [`pkg/schedule/affinity/parse.go`](../../../pkg/schedule/affinity/parse.go) | 71,72,95 | `a10c3a079011f6da2b60b00e8075a07a99eaf5921483e97ad93f51842fc3713e` |
| [`pkg/schedule/affinity/parse.rs`](../../../pkg/schedule/affinity/parse.rs) | 71,72,95 | `9c1b23e02cc8f7dd01fe95c8af09159ca2d8a734730567783d1ed30b9a71927f` |
| [`pkg/schedule/affinity/parse.rs.md`](../../../pkg/schedule/affinity/parse.rs.md) | 71,72,95 | `ea6bfc05cda7ba90c7b2e22358203b862be69bac08c07e97339fe84491e58663` |
| [`pkg/schedule/affinity/policy.go`](../../../pkg/schedule/affinity/policy.go) | 71,72,95 | `f2795183d03c3b045bdfdfbd3f5f59dccdf68428a75c268a66f34b06dc542eac` |
| [`pkg/schedule/affinity/policy.rs`](../../../pkg/schedule/affinity/policy.rs) | 71,72,95 | `e7e705dc5a5ba92ddc8aaa8e1a855428f7ba8ddd41b114c2d300905f855d9ba7` |
| [`pkg/schedule/affinity/policy.rs.md`](../../../pkg/schedule/affinity/policy.rs.md) | 71,72,95 | `c2ca9b4bc065aafbf259ac274b9f083a7c95e12baba90361534671ff8d290cfb` |
| [`pkg/schedule/affinity/policy_test.go`](../../../pkg/schedule/affinity/policy_test.go) | 71,72,95 | `4d37945d5d5f2d24ea5ad093c0c403d3bd375737cdf82ccf0d7bf2ce425bd2f0` |
| [`pkg/schedule/affinity/ranges.rs`](../../../pkg/schedule/affinity/ranges.rs) | 71,72,95 | `5dfc7053efd02699b1ab3b7ec46c213b71d39db7a9a9e8270d6d4a69e47119a9` |
| [`pkg/schedule/affinity/ranges.rs.md`](../../../pkg/schedule/affinity/ranges.rs.md) | 71,72,95 | `54929dcadeaa5197455be868b88a3032bc76c8ac318958bb085983d55bb11474` |
| [`pkg/schedule/affinity/region_cache.rs`](../../../pkg/schedule/affinity/region_cache.rs) | 71,72,95 | `c49d75796768f91275ff2f6bdb34dd943aaa7dfc200ddbb18fed82211263e126` |
| [`pkg/schedule/affinity/region_cache.rs.md`](../../../pkg/schedule/affinity/region_cache.rs.md) | 71,72,95 | `f6cf520a93f32367c79af7d217c3d058c88c35e9a36136b3f0d1736f49863617` |
| [`pkg/schedule/affinity/split.go`](../../../pkg/schedule/affinity/split.go) | 71,72,95 | `c4ff72da777c9d515bbb55dcbbf8ebc70d44e3257f214c49a83361a584d1e463` |
| [`pkg/schedule/affinity/split.rs`](../../../pkg/schedule/affinity/split.rs) | 71,72,95 | `f32109a8b2131306ce0822ed5092096427be462208c17ec907fbec8cdabef8d3` |
| [`pkg/schedule/affinity/split.rs.md`](../../../pkg/schedule/affinity/split.rs.md) | 71,72,95 | `efa452cf1e4ea886e85d6abe8717b0e4cb902e349d015663e7aa2773130096b5` |
| [`pkg/schedule/affinity/split_test.go`](../../../pkg/schedule/affinity/split_test.go) | 71,72,95 | `eb18736bfaa8bfbe7097b3aba74f447db593f9d49c320562f625f6055625262c` |
| [`pkg/schedule/affinity/txn.go`](../../../pkg/schedule/affinity/txn.go) | 71,72,95 | `26d83586a853bcef47525daba734a0aaaf821925e5f462fb79f84e75b6ed7ca4` |
| [`pkg/schedule/affinity/txn.rs`](../../../pkg/schedule/affinity/txn.rs) | 71,72,95 | `ce5448fc86701c1cc78f747e5df16a620bd2fedcba492130adbe9b5fc394bfa7` |
| [`pkg/schedule/affinity/txn.rs.md`](../../../pkg/schedule/affinity/txn.rs.md) | 71,72,95 | `54edc4acb44bf70d79e38865e2a98f591e1ee34a1566ef74770da5156302c182` |
| [`pkg/schedule/affinity/txn_peers.rs`](../../../pkg/schedule/affinity/txn_peers.rs) | 71,72,95 | `21be87c6db7a3a0519c3500a1790c4acfb7140eec479e0a42e707f2253f5f5f0` |
| [`pkg/schedule/affinity/txn_peers.rs.md`](../../../pkg/schedule/affinity/txn_peers.rs.md) | 71,72,95 | `89955727fb8bb75610678df270fdefc0d1a7f7bb59a6f25350475eb0af044b15` |
| [`pkg/schedule/affinity/txn_ranges.rs`](../../../pkg/schedule/affinity/txn_ranges.rs) | 71,72,95 | `3a5a74c917b300cc91c3351caef244f5b51607a99f57387fccd4048e16d38b77` |
| [`pkg/schedule/affinity/txn_ranges.rs.md`](../../../pkg/schedule/affinity/txn_ranges.rs.md) | 71,72,95 | `f12ee7f042a7ec973e2eb77bdf82900be741251e04b0eeeef1ff47c9e9700226` |
| [`pkg/schedule/affinity/txn_test.go`](../../../pkg/schedule/affinity/txn_test.go) | 71,72,95 | `5efe217b977e8df9da30203706e86356af9cdc2a7f816852dc320edf9cb0f527` |
| [`pkg/schedule/checker/Cargo.toml`](../../../pkg/schedule/checker/Cargo.toml) | 59,95 | `b03296810492d924c4d4bbc9b0c2c40fe4df0f844f8b918bae8751dbf99a2e3e` |
| [`pkg/schedule/checker/affinity_checker.go`](../../../pkg/schedule/checker/affinity_checker.go) | 59,95 | `e5b617a82f68abd5fda4fd30eb341a7bd588cb8187ff2cfef21446f8ee1bffbf` |
| [`pkg/schedule/checker/affinity_checker_test.go`](../../../pkg/schedule/checker/affinity_checker_test.go) | 59,95 | `fadc0e9679ec16020e95291f35c491b23a6a69d0b41e1eb9334c1a8f60446597` |
| [`pkg/schedule/checker/affinity_merge.rs`](../../../pkg/schedule/checker/affinity_merge.rs) | 59,95 | `9e30168cb3d25e61317b14368defe75aba5b6bd8e9ae2f9fe763f8f57263637e` |
| [`pkg/schedule/checker/affinity_merge.rs.md`](../../../pkg/schedule/checker/affinity_merge.rs.md) | 59,95 | `2b3c148cf12f2a93f6442f4ae6bc3f4f89c9a77ebc245d953628f531c4a29305` |
| [`pkg/schedule/checker/affinity_peer.rs`](../../../pkg/schedule/checker/affinity_peer.rs) | 59,95 | `c09828ca17f6ec11c6dbc7da6414efae779fcad4d2bf04c7da235a1b498d1219` |
| [`pkg/schedule/checker/affinity_peer.rs.md`](../../../pkg/schedule/checker/affinity_peer.rs.md) | 59,95 | `8e1c3789103038771097ffe2d9da6e36c6266bb52d95feb0a5e6b37a0e8f4b31` |
| [`pkg/schedule/checker/checker_controller.go`](../../../pkg/schedule/checker/checker_controller.go) | 59,95 | `48d03fd259b5762da0381ce8976057098d0b9d83a0527666c3bbc738862fd56d` |
| [`pkg/schedule/checker/checker_dispatch.rs`](../../../pkg/schedule/checker/checker_dispatch.rs) | 59,95 | `97df056c929527372cb17b6bd32db7b4dbcaaf06388006b6570fe8e3249874dc` |
| [`pkg/schedule/checker/checker_dispatch.rs.md`](../../../pkg/schedule/checker/checker_dispatch.rs.md) | 59,95 | `3ec452ef251f48419fd77b7740ad3ad488ff8307e04f7f113dd45ea9303d8ee6` |
| [`pkg/schedule/checker/checker_metrics.rs`](../../../pkg/schedule/checker/checker_metrics.rs) | 59,95 | `6449a77c434d9f5158f6f3fb14fbad46660b9813c15b2480869edd4125d60682` |
| [`pkg/schedule/checker/checker_metrics.rs.md`](../../../pkg/schedule/checker/checker_metrics.rs.md) | 59,95 | `f8e3fe4f0a903113bfd368b52534e90a87173447c2b8c30203e5228347e605e4` |
| [`pkg/schedule/checker/checker_pause.go`](../../../pkg/schedule/checker/checker_pause.go) | 59,95 | `c977895f300a94a87135be0203614aa9be867c1c1f98ddde7cd72dc9164815c6` |
| [`pkg/schedule/checker/joint_state_checker.go`](../../../pkg/schedule/checker/joint_state_checker.go) | 59,95 | `ffacb80b2f8420a33f3def7e78064a66ddf528639132a0e8ea2ea750e39f92be` |
| [`pkg/schedule/checker/joint_state_checker_test.go`](../../../pkg/schedule/checker/joint_state_checker_test.go) | 59,95 | `bef12b07b2d05a029efd8d08db6fdacb0bcf04e99340e2a0a0746da20577bad2` |
| [`pkg/schedule/checker/learner_checker.go`](../../../pkg/schedule/checker/learner_checker.go) | 59,95 | `99b7eadd147e3b5078a453575148c02ad3badcfa40283ed605dde2f74fba2349` |
| [`pkg/schedule/checker/learner_checker_test.go`](../../../pkg/schedule/checker/learner_checker_test.go) | 59,95 | `5b9f5282e3755d9be7b74610025e660d75bdfbc73765b3f58465d51b96905cc8` |
| [`pkg/schedule/checker/lib.rs`](../../../pkg/schedule/checker/lib.rs) | 59,95 | `0a18d1dcc28bb9118912dd021ffbc47e849c675e80c52220f415596656928bbc` |
| [`pkg/schedule/checker/lib.rs.md`](../../../pkg/schedule/checker/lib.rs.md) | 59,95 | `99e926dcd7ad6ef35e90203b594d5f7df7470630efadb31c3e0e9150e2613bf7` |
| [`pkg/schedule/checker/merge_checker.go`](../../../pkg/schedule/checker/merge_checker.go) | 59,95 | `cd5480a1c1a39e7bab3c10e51d4c9de7399e0e2a36a32614796c41f55f12aa75` |
| [`pkg/schedule/checker/merge_checker_test.go`](../../../pkg/schedule/checker/merge_checker_test.go) | 59,95 | `23c71ea5e72f71550dce735cab1f9f9ab8a4e25db6dcb76882adb9fdf858f0de` |
| [`pkg/schedule/checker/merge_decision.rs`](../../../pkg/schedule/checker/merge_decision.rs) | 59,95 | `5fdad7cdd517c6e3003539a55a73630d33d648c15c39fdb6306441c933efb5de` |
| [`pkg/schedule/checker/merge_decision.rs.md`](../../../pkg/schedule/checker/merge_decision.rs.md) | 59,95 | `bc808807cb662c366d628612ff6468b27db771d7d50101a306e21058c1bc4fcd` |
| [`pkg/schedule/checker/merge_guard.rs`](../../../pkg/schedule/checker/merge_guard.rs) | 59,95 | `30ae8e19f899f14c7496a6c5fafc12ca5f8535bfa2c6b1e607ea64bc606edd75` |
| [`pkg/schedule/checker/merge_guard.rs.md`](../../../pkg/schedule/checker/merge_guard.rs.md) | 59,95 | `d2daba40455263855c9b0deef45341b1347dcad135be8e100ea8b6a2e45e8010` |
| [`pkg/schedule/checker/metrics.go`](../../../pkg/schedule/checker/metrics.go) | 59,95 | `43494888f446b18223eb46c47c9f27fb0b4f75ba63c4d4fbf559c6d33e454cb5` |
| [`pkg/schedule/checker/patrol.rs`](../../../pkg/schedule/checker/patrol.rs) | 59,95 | `e65870269ce8de836b56b00898fd87e0ec795f12df9b4c9dd6c87ed87435483e` |
| [`pkg/schedule/checker/patrol.rs.md`](../../../pkg/schedule/checker/patrol.rs.md) | 59,95 | `c4bcddd721885a6a9a966c637e5757649b3304713407e72de3d20f00e774bdb9` |
| [`pkg/schedule/checker/priority_inspector.go`](../../../pkg/schedule/checker/priority_inspector.go) | 59,95 | `07a0b462eb215b6cd6902a22a12f306c0d51fff58adf582df0b3eb4ddcab51a2` |
| [`pkg/schedule/checker/priority_inspector.rs`](../../../pkg/schedule/checker/priority_inspector.rs) | 59,95 | `7130e909434ac58c52ac516f12fc55eb1939c18e98ebcbeff5edbd563b263c6c` |
| [`pkg/schedule/checker/priority_inspector.rs.md`](../../../pkg/schedule/checker/priority_inspector.rs.md) | 59,95 | `e71d0d6d0871d0a0364b8019b06497b01db6f7fd014fbfa9ef4652a5585dff96` |
| [`pkg/schedule/checker/priority_inspector_test.go`](../../../pkg/schedule/checker/priority_inspector_test.go) | 59,95 | `1df8b6627ebe6ce142ed60ca0ede27089d98d341568529b200ec17968e00cccc` |
| [`pkg/schedule/checker/replica_checker.go`](../../../pkg/schedule/checker/replica_checker.go) | 59,95 | `d0fd075ff8c7128cc1cac06857533acee19b99cb8da89ef3707b27f0cbccefa4` |
| [`pkg/schedule/checker/replica_checker_test.go`](../../../pkg/schedule/checker/replica_checker_test.go) | 59,95 | `45b842a3b3fd84e72c175115df9d4105753bd6a6d4be350c77e77f2be98fe75d` |
| [`pkg/schedule/checker/replica_fault.rs`](../../../pkg/schedule/checker/replica_fault.rs) | 59,95 | `8f4612cbff550ad852c410e8c8eaa087ef611c68cf97ce9db436bd74554a2046` |
| [`pkg/schedule/checker/replica_fault.rs.md`](../../../pkg/schedule/checker/replica_fault.rs.md) | 59,95 | `8216fcebbb55814498fb27e39be42a5dd47f086936994c22d458fc3c15c05390` |
| [`pkg/schedule/checker/replica_shape.rs`](../../../pkg/schedule/checker/replica_shape.rs) | 59,95 | `dcbe793569150c3cf60551ab2b1213b51e17d32f0715805c16251b3368534483` |
| [`pkg/schedule/checker/replica_shape.rs.md`](../../../pkg/schedule/checker/replica_shape.rs.md) | 59,95 | `db96d6ce310dd40a4e2ede78c232a5ce29bd278f95c35c4f2a59ff620df4d970` |
| [`pkg/schedule/checker/replica_strategy.go`](../../../pkg/schedule/checker/replica_strategy.go) | 59,95 | `0223f654c145abbcd6bf330cd42501766c415b85b60221692428ed13b7002c43` |
| [`pkg/schedule/checker/replica_strategy.rs`](../../../pkg/schedule/checker/replica_strategy.rs) | 59,95 | `16d24b3b3b4aeb7dc1f0ba48d634530fa0bbd1759c11af6853993219d1e16176` |
| [`pkg/schedule/checker/replica_strategy.rs.md`](../../../pkg/schedule/checker/replica_strategy.rs.md) | 59,95 | `b2398b92b5096bd4e741ca4c41c01cad1a57b62fd2531a5e2887d20d7a86711b` |
| [`pkg/schedule/checker/role_checkers.rs`](../../../pkg/schedule/checker/role_checkers.rs) | 59,95 | `1d8ec9ad3b89757419bfd174a8fad233b2ce8ea42a4c03820797048d1c9b2e66` |
| [`pkg/schedule/checker/role_checkers.rs.md`](../../../pkg/schedule/checker/role_checkers.rs.md) | 59,95 | `4f5595e23775a1d53716c8833bedfb1af4864975b7db717082ede2bd5502d1b9` |
| [`pkg/schedule/checker/rule_checker.go`](../../../pkg/schedule/checker/rule_checker.go) | 59,95 | `b2cccb0a8ac670aa5957a5728998e1a7cf4b35dff3f43dd1cfc4a699dae6fc61` |
| [`pkg/schedule/checker/rule_checker_test.go`](../../../pkg/schedule/checker/rule_checker_test.go) | 59,95 | `ac05aa4b43b98e29ca3772c35c152bc9748cc8ad34557d94a9b8050deeaeaee1` |
| [`pkg/schedule/checker/rule_fault.rs`](../../../pkg/schedule/checker/rule_fault.rs) | 59,95 | `ad78f81565473b62e2211e75f1b738166d3d41f3035122c277109be3a7e02228` |
| [`pkg/schedule/checker/rule_fault.rs.md`](../../../pkg/schedule/checker/rule_fault.rs.md) | 59,95 | `0ba2eda8a3bc59693dd6ee3e026d19c17230a6d842f6fc7962c3a3b38233c004` |
| [`pkg/schedule/checker/rule_location.rs`](../../../pkg/schedule/checker/rule_location.rs) | 59,95 | `8c5ba619d71a3da661bf11d5aff25514de60ec8e0ac1e6f4381f3488be5c97ec` |
| [`pkg/schedule/checker/rule_location.rs.md`](../../../pkg/schedule/checker/rule_location.rs.md) | 59,95 | `319861a129210d1850942b0c593874c1fe8d02893480ee0fa847e7d55d758e02` |
| [`pkg/schedule/checker/rule_missing.rs`](../../../pkg/schedule/checker/rule_missing.rs) | 59,95 | `07b64a53aa9b61a9ae8d0e754351a60227e42ee628d3a441845be0959a0215ec` |
| [`pkg/schedule/checker/rule_missing.rs.md`](../../../pkg/schedule/checker/rule_missing.rs.md) | 59,95 | `64fda475214c5ca29b0341fba18a0fb265114e8345f6dddb61d8f3f98e9e6906` |
| [`pkg/schedule/checker/split_checker.go`](../../../pkg/schedule/checker/split_checker.go) | 59,95 | `aa4b3e7558cde3f471e11335bcd24cac4b981682d785f602fae5cef50db0687c` |
| [`pkg/schedule/checker/split_checker.rs`](../../../pkg/schedule/checker/split_checker.rs) | 59,95 | `0059bd11bf0b321c4bb36a0897feb81d36186ee8f91ab6e4bbc2a9f82826e207` |
| [`pkg/schedule/checker/split_checker.rs.md`](../../../pkg/schedule/checker/split_checker.rs.md) | 59,95 | `2800a8453c5cf6cf01081e79f15f391a29b387fe3ea261a54afa19e62dd4a13b` |
| [`pkg/schedule/checker/split_checker_test.go`](../../../pkg/schedule/checker/split_checker_test.go) | 59,95 | `adcef649e2c1035e6bdd7490194ddde5347f443b61c79fb1697148f1b5f72d25` |
| [`pkg/schedule/checker/split_scatter.go`](../../../pkg/schedule/checker/split_scatter.go) | 59,95 | `840875adc09bc99b054242df78c776e7454a8e1a2bf1f22960e767684aa9baf8` |
| [`pkg/schedule/checker/split_scatter_group.go`](../../../pkg/schedule/checker/split_scatter_group.go) | 59,95 | `b2f8465dd84ab486d3b526c6d308899b774b933bc75655ba2d49b78d901a4159` |
| [`pkg/schedule/checker/split_scatter_group.rs`](../../../pkg/schedule/checker/split_scatter_group.rs) | 59,95 | `570c841a5380e7143b91f7eef4887c0b443430e94dd5bd82899085a4ce00b80e` |
| [`pkg/schedule/checker/split_scatter_group.rs.md`](../../../pkg/schedule/checker/split_scatter_group.rs.md) | 59,95 | `c1291e3dfb4f309f0de5e0e5879aa938add0224031af68977a705412219d28b7` |
| [`pkg/schedule/checker/split_scatter_observe.rs`](../../../pkg/schedule/checker/split_scatter_observe.rs) | 59,95 | `014372ff6cb2aa1ab4305ef56e9e5399b565b9ffea3c5036628e3984584450b4` |
| [`pkg/schedule/checker/split_scatter_observe.rs.md`](../../../pkg/schedule/checker/split_scatter_observe.rs.md) | 59,95 | `f6cd21a2d8cc736f0adfdcdee417ea32cf941b84d86b07c4d72b05d0b513e6a0` |
| [`pkg/schedule/checker/split_scatter_retry.rs`](../../../pkg/schedule/checker/split_scatter_retry.rs) | 59,95 | `3a3b6b2ccd221dde99e88847fceb9ba3046d995f7dc2cc44ec58c10b6a90cd80` |
| [`pkg/schedule/checker/split_scatter_retry.rs.md`](../../../pkg/schedule/checker/split_scatter_retry.rs.md) | 59,95 | `0f119d7d4b81440114947b736d9767aa73bb32bd7f3f09040f151ca83238416a` |
| [`pkg/schedule/checker/split_scatter_test.go`](../../../pkg/schedule/checker/split_scatter_test.go) | 59,95 | `a70221653dfb877244a7351b632f0ea485d2b99484af508e354939bb6d272e9c` |
| [`pkg/schedule/checker/suspect_ranges.rs`](../../../pkg/schedule/checker/suspect_ranges.rs) | 59,95 | `62b1261115c0bf60ff4861ef940161638a8edd656fc67f67075455df7e24d800` |
| [`pkg/schedule/checker/suspect_ranges.rs.md`](../../../pkg/schedule/checker/suspect_ranges.rs.md) | 59,95 | `816411a3e6fc2ce03a967c76009feeadc1fefc481770a70a5b8cec271e0df8a6` |
| [`pkg/schedule/checker/task_039_test.rs`](../../../pkg/schedule/checker/task_039_test.rs) | 59,95 | `f89b368207ddd082c22a12fb8eee0d72491f2f2f62566588ad00524eb29cfc89` |
| [`pkg/schedule/checker/task_040_test.rs`](../../../pkg/schedule/checker/task_040_test.rs) | 59,95 | `0967fd792daa4220186d36592059ca1ad8d846f011bcf8f34f179162de159783` |
| [`pkg/schedule/checker/task_041_test.rs`](../../../pkg/schedule/checker/task_041_test.rs) | 59,95 | `9f1f6d5dd9bdd2fa47afd6b284f944cc620665f074a368bcf4f4d5f4122f6522` |
| [`pkg/schedule/checker/task_042_test.rs`](../../../pkg/schedule/checker/task_042_test.rs) | 59,95 | `7d61c9ee78a85eb07aca1dff181afb4484beec2cec42f1f647443c2df694daf6` |
| [`pkg/schedule/checker/task_043_test.rs`](../../../pkg/schedule/checker/task_043_test.rs) | 59,95 | `740189536c9f233fe49078a6bf72669d25ee60a4a182f7a1408de7de51f861ee` |
| [`pkg/schedule/checker/task_044_test.rs`](../../../pkg/schedule/checker/task_044_test.rs) | 59,95 | `d5bf49f740843dd29308e2b7e5a6bd62d5a7dc130340554c951b6801bd277494` |
| [`pkg/schedule/checker/task_045_test.rs`](../../../pkg/schedule/checker/task_045_test.rs) | 59,95 | `1ca181d0c5e4caf137b1bdb1494ee5d5f8b4e4d701591e0b37c8a0ea866b35e2` |
| [`pkg/schedule/checker/task_046_test.rs`](../../../pkg/schedule/checker/task_046_test.rs) | 59,95 | `ffa45e79e9a02a09631ce843858041f7ac5de6e7ec37c745a4b08b7180de39fc` |
| [`pkg/schedule/checker/task_047_test.rs`](../../../pkg/schedule/checker/task_047_test.rs) | 59,95 | `bd71ce0cbb6f0329993d11284e7a5f3395191738611dea4043b3c1fb3833bc73` |
| [`pkg/schedule/checker/task_048_test.rs`](../../../pkg/schedule/checker/task_048_test.rs) | 59,95 | `eba819a15027b0b38f071948c09cec672018c58f4a2c2cb60be2e761ed1a43fe` |
| [`pkg/schedule/checker/task_049_test.rs`](../../../pkg/schedule/checker/task_049_test.rs) | 59,95 | `72c3b0ff38377b6de5c473a7c6c738e5aeb013b07520d23897b7a2c3a410e514` |
| [`pkg/schedule/checker/task_050_test.rs`](../../../pkg/schedule/checker/task_050_test.rs) | 59,95 | `e0ab491928448c30f5e65e4086ed91d3941ec35642f580fd4f02cb1420397bc0` |
| [`pkg/schedule/checker/task_051_test.rs`](../../../pkg/schedule/checker/task_051_test.rs) | 59,95 | `e59661b21b63c3ae4aa678d051fadc7dba40a7d58e6c791588c94db3f6fc5efc` |
| [`pkg/schedule/checker/task_052_test.rs`](../../../pkg/schedule/checker/task_052_test.rs) | 59,95 | `73116d95608500ee1bb2abe1d7322205364b24ea86aacc3e5cccf9023e852908` |
| [`pkg/schedule/checker/task_053_test.rs`](../../../pkg/schedule/checker/task_053_test.rs) | 59,95 | `b8157448c8a82e58b45db639c2bbc619cc9d1e91f387e9cc24e09c3a5de41239` |
| [`pkg/schedule/checker/task_054_test.rs`](../../../pkg/schedule/checker/task_054_test.rs) | 59,95 | `16daf36d32337a13760d49d2841939f6d45916b944475abffeb79a0064751a1f` |
| [`pkg/schedule/checker/task_055_test.rs`](../../../pkg/schedule/checker/task_055_test.rs) | 59,95 | `e49fcbf85788c1118566503d4a58636bb32208846edf3627f58a35a18a7023d9` |
| [`pkg/schedule/checker/task_056_test.rs`](../../../pkg/schedule/checker/task_056_test.rs) | 59,95 | `e1c903e29747cebc11fa2cbf6d8eb45863c7912b30ee5a8ac69b8142fa4feea3` |
| [`pkg/schedule/checker/task_057_test.rs`](../../../pkg/schedule/checker/task_057_test.rs) | 59,95 | `bd65f04df3097a75e38398060525799b6346c41e91a10d2e6c246a44fe9418b9` |
| [`pkg/schedule/checker/task_058_test.rs`](../../../pkg/schedule/checker/task_058_test.rs) | 59,95 | `5db32ef5aa86e41c218c8812e19e24914dbd49fd5598d672794a6399767e179b` |
| [`pkg/schedule/config/Cargo.toml`](../../../pkg/schedule/config/Cargo.toml) | 95 | `87613df49684f3b4c689db4517010a207156e69bde6815d47860f3ecba15f9b8` |
| [`pkg/schedule/config/OWNERS`](../../../pkg/schedule/config/OWNERS) | 95 | `e8945fe66eafe0e6cedbf27ad1c180808b250fbb8a47835d221d7d2626071e05` |
| [`pkg/schedule/config/config.go`](../../../pkg/schedule/config/config.go) | 95 | `33c0cce0ec0c3feb1b789a0366584433ad104786ecbab29edd2224d413398557` |
| [`pkg/schedule/config/config_provider.go`](../../../pkg/schedule/config/config_provider.go) | 95 | `c9b89f57cd68ef3eeca260a0cb53883cfc9c32ad3ea375fa81438adaade280e5` |
| [`pkg/schedule/config/config_provider_test.rs`](../../../pkg/schedule/config/config_provider_test.rs) | 95 | `cf9d21daeaa9a37f652066935ad5e6b8f9eb32a3552eba5e8379da5603d7dfe8` |
| [`pkg/schedule/config/config_test.rs`](../../../pkg/schedule/config/config_test.rs) | 95 | `1517300f8d7d58e14a0b641a21e3fac33073a6aa1ddc66fd5f1157ee7486fcec` |
| [`pkg/schedule/config/config_values_test.rs`](../../../pkg/schedule/config/config_values_test.rs) | 95 | `8a23f39e96a4d7c8ad9605c72f9635f078c89729398216fb1fb1503cd4d8cda9` |
| [`pkg/schedule/config/lib.rs`](../../../pkg/schedule/config/lib.rs) | 95 | `c65938bfb0c9623234aaf4855a5cf022f6cb3914d61e65444d89007731424e2e` |
| [`pkg/schedule/config/provider.rs`](../../../pkg/schedule/config/provider.rs) | 95 | `60044585b818c3ddcb993f1d400d8924fa2df1ec6a983c9320a90b43cff0da56` |
| [`pkg/schedule/config/provider.rs.md`](../../../pkg/schedule/config/provider.rs.md) | 95 | `97338d9c596fa2492ea1a63e24d49c152d40ef17b20ad25edc7c6f4245a87d68` |
| [`pkg/schedule/config/store_config.go`](../../../pkg/schedule/config/store_config.go) | 95 | `81060941f0cee6dbd46d052ff1f730cd236d04211155f2c28a43f7fb884596e9` |
| [`pkg/schedule/config/store_config.rs`](../../../pkg/schedule/config/store_config.rs) | 95 | `d8e21cac1bb2fb18c958179d22cfee254a5d93e1f153a52986f88fb49b945c08` |
| [`pkg/schedule/config/store_config.rs.md`](../../../pkg/schedule/config/store_config.rs.md) | 95 | `9efe21cae58c381bb7e72b35a5e520a7f562083925971ad46d4cf0a6a19029b0` |
| [`pkg/schedule/config/store_config_test.go`](../../../pkg/schedule/config/store_config_test.go) | 95 | `f4114a41ede0c192a243b38e0ac4acff357ab7458c8f050068d269925ee79dc4` |
| [`pkg/schedule/config/store_config_test.rs`](../../../pkg/schedule/config/store_config_test.rs) | 95 | `09bb4135390dd09d5edee4632e1b08bfbcdce2acfd5c2372cccc8b87e5d74dfa` |
| [`pkg/schedule/config/util.go`](../../../pkg/schedule/config/util.go) | 95 | `12aa3025ba33fd0752d527e24a5c07d48e962caa7bff95325be4fabcd453f838` |
| [`pkg/schedule/config/util_test.go`](../../../pkg/schedule/config/util_test.go) | 95 | `158f0467ed31cbc97a0ba353fab94fba88fdcf01f610dbd79b83555251bfd1b2` |
| [`pkg/schedule/config/value_contract.rs`](../../../pkg/schedule/config/value_contract.rs) | 95 | `0239a53a8170203c329a6e61abedabe4b027cdaa51eb512dc1b368250a9fedf7` |
| [`pkg/schedule/config/value_contract.rs.md`](../../../pkg/schedule/config/value_contract.rs.md) | 95 | `bd6972d9bd9850974966e5a0262808240e948c634871761acdf39c9fae462a46` |
| [`pkg/schedule/coordinator.go`](../../../pkg/schedule/coordinator.go) | 12,13,14,15,16,17,95 | `a61e5be78180e4a9e3f385beb6354ed352915785771edcc3a5805ee271cc5fc2` |
| [`pkg/schedule/coordinator.rs`](../../../pkg/schedule/coordinator.rs) | 17,29,31,64,67,95 | `00bde629da398f546fa74a12ac76f6a86cf6751d44e04620c0bbc7186c6039c3` |
| [`pkg/schedule/coordinator.rs.md`](../../../pkg/schedule/coordinator.rs.md) | 95 | `c768bdb28bc1ad6c46b7f56695a8f3f350bc576b578c856c0d22867246860f15` |
| [`pkg/schedule/coordinator_lifecycle.rs`](../../../pkg/schedule/coordinator_lifecycle.rs) | 95 | `c2bf222fbad715349778f16aa0240843b31ce776dd1179708594974f66105006` |
| [`pkg/schedule/coordinator_lifecycle.rs.md`](../../../pkg/schedule/coordinator_lifecycle.rs.md) | 95 | `1ac2b9c5148d9f24b7bc49e62654c8b1d1f294e61266e346aac847f03e126df7` |
| [`pkg/schedule/coordinator_observe.rs`](../../../pkg/schedule/coordinator_observe.rs) | 95 | `a1eec179e0755a64f279e6e32f37786173c1b80d9cd4dff4e47eb9cbafe86548` |
| [`pkg/schedule/coordinator_observe.rs.md`](../../../pkg/schedule/coordinator_observe.rs.md) | 95 | `19863a88cc438eb50679c47c79147e3509a4dfbc64fa7613e10ea99ecb2f2756` |
| [`pkg/schedule/core/Cargo.toml`](../../../pkg/schedule/core/Cargo.toml) | 95 | `f66bab2626d8bf670a872105a547d414ac4d0112517fae26551b3caf52b1c94a` |
| [`pkg/schedule/core/basic_cluster.rs`](../../../pkg/schedule/core/basic_cluster.rs) | 95 | `39a8a02027193ec8cbc4073f32f00e75645218100fe95184753d06628f84b2a9` |
| [`pkg/schedule/core/basic_cluster.rs.md`](../../../pkg/schedule/core/basic_cluster.rs.md) | 95 | `d2b7a9ab49c45fcf60b5041d497cf7b618cf533b09313e1a30e043cf52ddea67` |
| [`pkg/schedule/core/cluster_informer.go`](../../../pkg/schedule/core/cluster_informer.go) | 95 | `657dd1208bb79c0ffd6820d8bdc558b6ca17f15cba4917e86d325978344bfe78` |
| [`pkg/schedule/core/cluster_views.rs`](../../../pkg/schedule/core/cluster_views.rs) | 95 | `3b39d4485078318365fad641a4df7bcab8ef9734c79470a2e97335f120cd42e2` |
| [`pkg/schedule/core/cluster_views.rs.md`](../../../pkg/schedule/core/cluster_views.rs.md) | 95 | `195ec3ab4aea20fb3f57de5450553f8ccc4f945960364b21729d4576e91a0780` |
| [`pkg/schedule/core/lib.rs`](../../../pkg/schedule/core/lib.rs) | 95 | `531f9a163accd5999bb2b27a05e987d43962004a3bee0bbdfca7c9cfd6181b14` |
| [`pkg/schedule/core/shared_cluster.rs`](../../../pkg/schedule/core/shared_cluster.rs) | 95 | `bfd12d844c5a87d08f5d25108426e97fee3f760847695ef0f44ac11bddcac190` |
| [`pkg/schedule/core/shared_cluster.rs.md`](../../../pkg/schedule/core/shared_cluster.rs.md) | 95 | `67ee1f410c9f26c6bac2d37f62fb4dbc6ad0ddd5f41d492f47197d1748eafc94` |
| [`pkg/schedule/core/task_002_test.rs`](../../../pkg/schedule/core/task_002_test.rs) | 95 | `53cd933d33608f4f441066463d799c09c728ba500e6130b871b711c68f26ea45` |
| [`pkg/schedule/core/task_003_test.rs`](../../../pkg/schedule/core/task_003_test.rs) | 95 | `5f36df7625f1bfb3bcf4c4cde6cd6c80434f4c251e70795296e3caf871ec2a01` |
| [`pkg/schedule/core/task_004_test.rs`](../../../pkg/schedule/core/task_004_test.rs) | 95 | `78dccd57113155b1f637ecc9ab8144bc3a1c389b0e45fbe283417086861ee32f` |
| [`pkg/schedule/diagnostic/Cargo.toml`](../../../pkg/schedule/diagnostic/Cargo.toml) | 66,95 | `c66a3f1b6c6a1d6d1565f734f6a69ea8c30078685f0b970d2fbe9e47a6ba574f` |
| [`pkg/schedule/diagnostic/diagnostic.rs`](../../../pkg/schedule/diagnostic/diagnostic.rs) | 66,95 | `2978b8721f74df4ec60df8e2e0fedcf8a6d55bf01fed707bfd042dff05e6b574` |
| [`pkg/schedule/diagnostic/diagnostic.rs.md`](../../../pkg/schedule/diagnostic/diagnostic.rs.md) | 66,95 | `f0d07843fbb418fec195b181988eccd813601dd14a34f7086ecd6b2ab117bed9` |
| [`pkg/schedule/diagnostic/diagnostic_manager.go`](../../../pkg/schedule/diagnostic/diagnostic_manager.go) | 66,95 | `bd39573ab8ea807b4eb689f875a3aeed573b02bf77406f1d18eb33578003ee1e` |
| [`pkg/schedule/diagnostic/lib.rs`](../../../pkg/schedule/diagnostic/lib.rs) | 66,95 | `cda8b18db8c02dd82337edfa08419c720263fb2b93438f70717a317bb5b9656a` |
| [`pkg/schedule/diagnostic/task_111_test.rs`](../../../pkg/schedule/diagnostic/task_111_test.rs) | 66,95 | `b13febc71abd5aba63b12aebeaa4107d989006066d3a33a84b6fec0b64c2c053` |
| [`pkg/schedule/filter/Cargo.toml`](../../../pkg/schedule/filter/Cargo.toml) | 95 | `5078e1671cbd7100fa2f593a968d9732566f8e87d72aedc750a894686740751d` |
| [`pkg/schedule/filter/candidates.go`](../../../pkg/schedule/filter/candidates.go) | 95 | `2ee5704d3f38c6a5675a9654409bd5df1532ad019dd30bdfd1105315c5f9ac12` |
| [`pkg/schedule/filter/candidates.rs`](../../../pkg/schedule/filter/candidates.rs) | 95 | `7b9b35b40aaf16f2e6dff3a44d2e7eb97ac290d57486b3ce78f017a754d1c045` |
| [`pkg/schedule/filter/candidates.rs.md`](../../../pkg/schedule/filter/candidates.rs.md) | 95 | `322f75f7e6751d8b00f709f353367029f3f12df7325f3fa0c7c721250ffceca1` |
| [`pkg/schedule/filter/candidates_test.go`](../../../pkg/schedule/filter/candidates_test.go) | 95 | `f40241c4defd8b264abfe01f042352d3703602814331fd89b5c5c2c285622ed0` |
| [`pkg/schedule/filter/comparer.go`](../../../pkg/schedule/filter/comparer.go) | 95 | `acbce00d2e9492b121818972f28af831534f7fad2e4e2b97fb8f0ef1429d294d` |
| [`pkg/schedule/filter/counter.go`](../../../pkg/schedule/filter/counter.go) | 95 | `a291b8960207ce0af627bf83bcd2730043c969520d1804caef8a788ef7e2c809` |
| [`pkg/schedule/filter/counter_test.go`](../../../pkg/schedule/filter/counter_test.go) | 95 | `b761260498e2d42cb4e81b5a7e80de66194bc7e3f995e432bbcccd384229f61e` |
| [`pkg/schedule/filter/filter_metrics.rs`](../../../pkg/schedule/filter/filter_metrics.rs) | 95 | `f69e8d2e0a36841d2cd0ee2ff1be8b4cd6ea04a59f5057cf8c6c9c8150b6e8a0` |
| [`pkg/schedule/filter/filter_metrics.rs.md`](../../../pkg/schedule/filter/filter_metrics.rs.md) | 95 | `8c26dd3ebbfc94597baeb3658f80ccc8154f6b13ac6fce7e229b8b70da1731da` |
| [`pkg/schedule/filter/filters.go`](../../../pkg/schedule/filter/filters.go) | 95 | `2e72100da29ddac7d371c59898ef10a562cda9bf4eec666cc4e4e0c7c8edc0a4` |
| [`pkg/schedule/filter/filters_test.go`](../../../pkg/schedule/filter/filters_test.go) | 95 | `7675a55b6d20d983416b5b0d0e215fa0e1033f5dfc0367f73c0e73f7e9f52e05` |
| [`pkg/schedule/filter/healthy.go`](../../../pkg/schedule/filter/healthy.go) | 95 | `dc9137941fc887c15aaf744650235939e7c28704958e3b1c8d2c3970a5502c5d` |
| [`pkg/schedule/filter/healthy_test.go`](../../../pkg/schedule/filter/healthy_test.go) | 95 | `a476c05b0daff9784a69df42a6fc8532ab76b44b8bdc73cfc9ad90f5e0fb0577` |
| [`pkg/schedule/filter/lib.rs`](../../../pkg/schedule/filter/lib.rs) | 95 | `37fb8954a7d1977f57c8959b0d5800ef036cc7c8f26a2024922f4fe514b0053e` |
| [`pkg/schedule/filter/metrics.go`](../../../pkg/schedule/filter/metrics.go) | 95 | `2b9fcd6a5491be91ec05e88d3515f798eabfdbe8dc8b8fdc4d7e73bbd9073788` |
| [`pkg/schedule/filter/placement_guard.rs`](../../../pkg/schedule/filter/placement_guard.rs) | 95 | `a5fe31133f6e3addf30b5f15fd465f588dc8acddcb1e4fee061c9cc127551611` |
| [`pkg/schedule/filter/placement_guard.rs.md`](../../../pkg/schedule/filter/placement_guard.rs.md) | 95 | `cdb5fea06a4fa18e29dd077837e29fb323655c38a4af9965a8a8bacbc4c80d5a` |
| [`pkg/schedule/filter/region_filters.go`](../../../pkg/schedule/filter/region_filters.go) | 95 | `ab08b272ad1e1ff1f044e40ba0091f13678d3d31f09bbf9c61b2a009f8903915` |
| [`pkg/schedule/filter/region_filters_test.go`](../../../pkg/schedule/filter/region_filters_test.go) | 95 | `5ad85ccafba024240ee68bd4e83d97ad7c4c79dade3813e24f1800ffa1944bf5` |
| [`pkg/schedule/filter/region_health.rs`](../../../pkg/schedule/filter/region_health.rs) | 95 | `cf17cee9f0ac48f372fd93f334b2ece1a184b30efdd2626befc8a63550f4e524` |
| [`pkg/schedule/filter/region_health.rs.md`](../../../pkg/schedule/filter/region_health.rs.md) | 95 | `b494f16a42ea9579e7b391b80578f982277cd41f237242fb712bdad4817abafc` |
| [`pkg/schedule/filter/selection.rs`](../../../pkg/schedule/filter/selection.rs) | 95 | `94afcde49a67b06cc7208a7c1273d02ff0742299878e4367a2681ae4325701a5` |
| [`pkg/schedule/filter/selection.rs.md`](../../../pkg/schedule/filter/selection.rs.md) | 95 | `7f8d0d4daf15a6a9dadfb8ef2c8afee0385f3f997068a9277b1d1c2d578f0aee` |
| [`pkg/schedule/filter/status.go`](../../../pkg/schedule/filter/status.go) | 95 | `9517ad53b6b90dd79634d4eb37548e9c72cdc8b26ee41427ec5b562bdac51dfd` |
| [`pkg/schedule/filter/store_constraints.rs`](../../../pkg/schedule/filter/store_constraints.rs) | 95 | `d19091da99b42dc03c932475367ca80fe4702fb03f4e0fa954ec11b595ba0b49` |
| [`pkg/schedule/filter/store_constraints.rs.md`](../../../pkg/schedule/filter/store_constraints.rs.md) | 95 | `c1f70c753cf047bbaf83200f7be24861e5e2de724dd180c887e3324a9e6969b3` |
| [`pkg/schedule/filter/store_state.rs`](../../../pkg/schedule/filter/store_state.rs) | 95 | `732e9246c8f4d2ac21b69c405bcb1ef2ff6304aa611081c4a931a3de61cdf0e3` |
| [`pkg/schedule/filter/store_state.rs.md`](../../../pkg/schedule/filter/store_state.rs.md) | 95 | `100bf967438ad595f5d37744e755168cb6e10694efcffe27764dbc86caceb782` |
| [`pkg/schedule/filter/task_005_test.rs`](../../../pkg/schedule/filter/task_005_test.rs) | 95 | `7852fc01d71fec9005cf51f624a58ea025617f080e3adb934805c7e17ffe65cf` |
| [`pkg/schedule/filter/task_006_test.rs`](../../../pkg/schedule/filter/task_006_test.rs) | 95 | `3c0f4b95ee0064fc8cfee273bb66eb4cc40644026cc1a537be983ba5bacae7ad` |
| [`pkg/schedule/filter/task_007_test.rs`](../../../pkg/schedule/filter/task_007_test.rs) | 95 | `c1785760ff31a9b5e681ba22d9b0b6b7cd0c1868c2fe26bf7d5f83c7505ad475` |
| [`pkg/schedule/filter/task_008_test.rs`](../../../pkg/schedule/filter/task_008_test.rs) | 95 | `de177599a5e23c83b56553274ee0cf8112548a929d8b2fbf1e4de5fe40d425c0` |
| [`pkg/schedule/filter/task_009_test.rs`](../../../pkg/schedule/filter/task_009_test.rs) | 95 | `f149971213dffd50001372a02761320af5af56eaef7ac1f0803664f79fce05ad` |
| [`pkg/schedule/filter/task_010_test.rs`](../../../pkg/schedule/filter/task_010_test.rs) | 95 | `4425f3a23442d3106f95aa3a472d09b46ff9ea57cbeec47d5f35963f198b189d` |
| [`pkg/schedule/filter/task_011_test.rs`](../../../pkg/schedule/filter/task_011_test.rs) | 95 | `7fa1d149b5c97176439925da15b8af72c56fe6fc0a45b688aaae3cd1c9e0871b` |
| [`pkg/schedule/handler/Cargo.toml`](../../../pkg/schedule/handler/Cargo.toml) | 63,95 | `6dbaa58bd2bc129475f49c596a7176755335af53b31195182bbd6ad057ce0089` |
| [`pkg/schedule/handler/admin_parse.rs`](../../../pkg/schedule/handler/admin_parse.rs) | 63,95 | `1f0dcf814e4f5a77b8fb8be7be38c22f8264894b89baa2f107e6fb54c406642a` |
| [`pkg/schedule/handler/admin_parse.rs.md`](../../../pkg/schedule/handler/admin_parse.rs.md) | 63,95 | `5ba081a8c1d03a3e3db61e71b222d14c4fedb05ce50333eba5dfa53bb88b4684` |
| [`pkg/schedule/handler/admin_peer.rs`](../../../pkg/schedule/handler/admin_peer.rs) | 63,95 | `857a113b33d42184abbbff1614cd38300689495ef441eda63222e5058cddcd80` |
| [`pkg/schedule/handler/admin_peer.rs.md`](../../../pkg/schedule/handler/admin_peer.rs.md) | 63,95 | `452ec2e9148c4f3884288f189b4b4f3803bee4d8a4186c1951d78841537ef2d1` |
| [`pkg/schedule/handler/admin_range.rs`](../../../pkg/schedule/handler/admin_range.rs) | 63,95 | `f3336d7b03db0f95e03434c0615b2f6f53ae619e6b6fc9d4876b29e9d61e67ca` |
| [`pkg/schedule/handler/admin_range.rs.md`](../../../pkg/schedule/handler/admin_range.rs.md) | 63,95 | `3855ff456bae4fc723ef70db0b7921b6cb84bb3eb50bcaf98776a0f676b122c5` |
| [`pkg/schedule/handler/handler.go`](../../../pkg/schedule/handler/handler.go) | 63,95 | `c9f126a421b83fdee5b89d4c26a743b2c4d6493527d4ba0862895705f3639dfa` |
| [`pkg/schedule/handler/hot_query.rs`](../../../pkg/schedule/handler/hot_query.rs) | 63,95 | `05741ba8451844c5d57e44d583f9534809e663573d7f5fa6d75502b81f689fa1` |
| [`pkg/schedule/handler/hot_query.rs.md`](../../../pkg/schedule/handler/hot_query.rs.md) | 63,95 | `4c014924ca187d293ca743e26e59561547b2b472be6307a370ae36453225bac0` |
| [`pkg/schedule/handler/lib.rs`](../../../pkg/schedule/handler/lib.rs) | 63,95 | `972d4fbc3c05f1cee4dfd718d4cebb54a3876bea835180c3222f74ef014d25ae` |
| [`pkg/schedule/handler/lib.rs.md`](../../../pkg/schedule/handler/lib.rs.md) | 63,95 | `48ec28946c3fa866d4a2a4c5335ac61df26933ad9a659d8bf1d533c9c478127f` |
| [`pkg/schedule/handler/operator_query.rs`](../../../pkg/schedule/handler/operator_query.rs) | 63,95 | `fd81dbb1aa843e828d6897221ffd2cb1aa54806d5f5a8cdbf39c2f1b03aee75a` |
| [`pkg/schedule/handler/operator_query.rs.md`](../../../pkg/schedule/handler/operator_query.rs.md) | 63,95 | `6fe01cab01b8d5291af615d0dd434142d205b3275a221ee862e2af86bb1f23b8` |
| [`pkg/schedule/handler/region_admin.rs`](../../../pkg/schedule/handler/region_admin.rs) | 63,95 | `10fd585919eebde416db5fcdbb4ba649bf375d9095717e6cc9ac0223be6d31e3` |
| [`pkg/schedule/handler/region_admin.rs.md`](../../../pkg/schedule/handler/region_admin.rs.md) | 63,95 | `d7c690403e12faa815ce5ae53dd65cd5e189633be4c70d04457dbbac237c9951` |
| [`pkg/schedule/handler/scatter_admin.rs`](../../../pkg/schedule/handler/scatter_admin.rs) | 63,95 | `253f311cde755f3f359605afdec1a311dc686cc06f024b0659d24b053b386401` |
| [`pkg/schedule/handler/scatter_admin.rs.md`](../../../pkg/schedule/handler/scatter_admin.rs.md) | 63,95 | `97e4edb99c96e0b8d6d3ca918397bd0a0ba1424cd291612cc901f5d025725b7b` |
| [`pkg/schedule/handler/scheduler_admin.rs`](../../../pkg/schedule/handler/scheduler_admin.rs) | 63,95 | `7754b70084cc6716f28d3f70cc9be37793e4e09e205b1c18725a2801da6c97be` |
| [`pkg/schedule/handler/scheduler_admin.rs.md`](../../../pkg/schedule/handler/scheduler_admin.rs.md) | 63,95 | `72fd0b65dacb2f60be44daf9ecabb43fc70bf93e2d7d09840fba7537c0a36740` |
| [`pkg/schedule/handler/split_admin.rs`](../../../pkg/schedule/handler/split_admin.rs) | 63,95 | `bcadfc2d8b234f3bf0002e1b192654289d7bdaaa27771f4eac50580080d9b643` |
| [`pkg/schedule/handler/split_admin.rs.md`](../../../pkg/schedule/handler/split_admin.rs.md) | 63,95 | `5df880974de4e9c6cdabcb767502e9d4a4052f0e4ad2967e451f178a98c0a44c` |
| [`pkg/schedule/handler/task_118_test.rs`](../../../pkg/schedule/handler/task_118_test.rs) | 63,95 | `260363e9f77a8b8a9c41f253cf756c54c7bf18f1472071976b73e9303801f21d` |
| [`pkg/schedule/handler/task_119_test.rs`](../../../pkg/schedule/handler/task_119_test.rs) | 63,95 | `24e72df5cdef166fe6770ac1cc29b7a5329a4fb0bf92cdff0d4c8f2b56203312` |
| [`pkg/schedule/handler/task_120_test.rs`](../../../pkg/schedule/handler/task_120_test.rs) | 63,95 | `656542735da5029a1e776a77c991a035e392a4e843bc55c52e85e33ddb6f0627` |
| [`pkg/schedule/handler/task_121_test.rs`](../../../pkg/schedule/handler/task_121_test.rs) | 63,95 | `a9a8763d335b4c3c1b0a6933d0fc3c1139480a164f9068beeaf114cd1c3870c7` |
| [`pkg/schedule/handler/task_122_test.rs`](../../../pkg/schedule/handler/task_122_test.rs) | 63,95 | `1814b9a8dd1d5039ba1d3ca6c17afc07b7210fe986c97f02320b84ff5743fe7b` |
| [`pkg/schedule/handler/task_123_test.rs`](../../../pkg/schedule/handler/task_123_test.rs) | 63,95 | `06dee1804489a863246acad4fc35eff2b2e4f69b6d4d438041480f4d79d2791a` |
| [`pkg/schedule/handler/task_124_test.rs`](../../../pkg/schedule/handler/task_124_test.rs) | 63,95 | `b242c3be3c4422a911c63711b55958a1dc320f9bf88a5eb6deed22d5a74e57cd` |
| [`pkg/schedule/handler/task_125_test.rs`](../../../pkg/schedule/handler/task_125_test.rs) | 63,95 | `1d59d8c63d408df6cb41742e36ac0c6e100a174e0dae9b5e6caef30ff9ce06aa` |
| [`pkg/schedule/handler/task_126_test.rs`](../../../pkg/schedule/handler/task_126_test.rs) | 63,95 | `264c714c19c0a115982ffbfa6829543d948a5d9a84a45ee5d71671abd8a53ded` |
| [`pkg/schedule/hbstream/Cargo.toml`](../../../pkg/schedule/hbstream/Cargo.toml) | 95 | `c8b7e7a5f4dd409ffb4b8d9908a290e995124723af5df10e37b5921180ec24f1` |
| [`pkg/schedule/hbstream/heartbeat_streams.go`](../../../pkg/schedule/hbstream/heartbeat_streams.go) | 95 | `e9388f1dd8e1ab906c273df018a2446b36450f6894bdc3ad2e0a70ec0a999489` |
| [`pkg/schedule/hbstream/lib.rs`](../../../pkg/schedule/hbstream/lib.rs) | 95 | `ecb7c4244d7746ef4cf0e45c1218fbc914de4a89f9cf5c5de7f3fbe433af5866` |
| [`pkg/schedule/hbstream/metric.go`](../../../pkg/schedule/hbstream/metric.go) | 95 | `ff4b8689ae09cac77e99f529681d0624f94400b564d6b08329c83a782517267d` |
| [`pkg/schedule/hbstream/operation.rs`](../../../pkg/schedule/hbstream/operation.rs) | 95 | `50b47f4cafea29478f43c502228e13f37a1afa0625bae7fbcfcd3afb8125fe99` |
| [`pkg/schedule/hbstream/operation.rs.md`](../../../pkg/schedule/hbstream/operation.rs.md) | 95 | `7222c4f2423d550ccb75eb7f670735903d6f52d7053f563f3b34f8aefcc8ed66` |
| [`pkg/schedule/hbstream/routing.rs`](../../../pkg/schedule/hbstream/routing.rs) | 95 | `4fa1634d3a1783f54c8de654344cb58d8954ff50df8ec0e9d0446cd97bdc0c05` |
| [`pkg/schedule/hbstream/routing.rs.md`](../../../pkg/schedule/hbstream/routing.rs.md) | 95 | `f810335dab8c17bb32e00fb34d116af9002a13ece6c69177c415871288634500` |
| [`pkg/schedule/hbstream/stream_lifecycle.rs`](../../../pkg/schedule/hbstream/stream_lifecycle.rs) | 95 | `f73f7edcdde7055f56073e7ede30e3719b304ee2bd6c8f99b2c8599cad129f04` |
| [`pkg/schedule/hbstream/stream_lifecycle.rs.md`](../../../pkg/schedule/hbstream/stream_lifecycle.rs.md) | 95 | `f25d5f4453870fe41261c78cacd6d7c3769b29247bcbffbe9cfec057f4a8a10f` |
| [`pkg/schedule/hbstream/task_012_test.rs`](../../../pkg/schedule/hbstream/task_012_test.rs) | 95 | `26121a479b984e6b617c6f8a65103f152cf7df5183a7131edbe7aef4a51380fd` |
| [`pkg/schedule/hbstream/task_013_test.rs`](../../../pkg/schedule/hbstream/task_013_test.rs) | 95 | `087c0a9a853ff95d14c5943913d75d3735de74766f132e02e4fe190bd32050bf` |
| [`pkg/schedule/hbstream/task_014_test.rs`](../../../pkg/schedule/hbstream/task_014_test.rs) | 95 | `23ef787eecf3a65eac3ccc397718d08b90501c947efbbfc0034b81d3f003ed35` |
| [`pkg/schedule/keyrange/Cargo.toml`](../../../pkg/schedule/keyrange/Cargo.toml) | 95 | `1e2839bd5002b6948df307377be616d5c9f35952b0777d2b6bd57050c0ee4195` |
| [`pkg/schedule/keyrange/lib.rs`](../../../pkg/schedule/keyrange/lib.rs) | 95 | `b9253003e6c8f4da6465deef24ab98aaa1e06d77760238cc2bea6b4949202d92` |
| [`pkg/schedule/keyrange/manager.go`](../../../pkg/schedule/keyrange/manager.go) | 95 | `8d44a24d69e190dc63ae9b92d092a3b384c1662570db865961d98a653e03a985` |
| [`pkg/schedule/keyrange/manager.rs`](../../../pkg/schedule/keyrange/manager.rs) | 95 | `d5db56b29fda93dd63af627cd9003f1828d335bfe552f1b521a6037983e65c30` |
| [`pkg/schedule/keyrange/manager.rs.md`](../../../pkg/schedule/keyrange/manager.rs.md) | 95 | `678543b0deddb7144d325c2fd4541e9611d853224f724771de7d1d1fcd42af43` |
| [`pkg/schedule/keyrange/range_append_test.rs`](../../../pkg/schedule/keyrange/range_append_test.rs) | 95 | `fd0953673e28745a5b147e1781bd20fce0a18b91801aea852a8f051185348a9f` |
| [`pkg/schedule/keyrange/range_subtract_test.rs`](../../../pkg/schedule/keyrange/range_subtract_test.rs) | 95 | `bd1b8453dd3a3aac4fb600f92b43dd3297441738c57282d650939b2497fb6740` |
| [`pkg/schedule/labeler/Cargo.toml`](../../../pkg/schedule/labeler/Cargo.toml) | 70,95 | `643167d68bd1a3924a0c13fd4db3411209ab0866c980b9951f39a66af8120fe7` |
| [`pkg/schedule/labeler/gc.rs`](../../../pkg/schedule/labeler/gc.rs) | 70,95 | `bcc68ebb3d751f2c400f376f3b2fb66bb2187eca94aafb2e53c2c942a8c3a3dd` |
| [`pkg/schedule/labeler/gc.rs.md`](../../../pkg/schedule/labeler/gc.rs.md) | 70,95 | `8686b077c2241a7c12ccb6124f349a9c9330e02ddb279074543fab28210f6155` |
| [`pkg/schedule/labeler/keyspace_index.go`](../../../pkg/schedule/labeler/keyspace_index.go) | 70,95 | `0a1679ac760ad78b6a0fdf70f77f3913943e3c2bbe0e18ac5bd5c43407adcf93` |
| [`pkg/schedule/labeler/keyspace_index.rs`](../../../pkg/schedule/labeler/keyspace_index.rs) | 70,95 | `491c129f8f3953840b12ea4496b85c9944d07f927020538d807682500ac898a3` |
| [`pkg/schedule/labeler/keyspace_index.rs.md`](../../../pkg/schedule/labeler/keyspace_index.rs.md) | 70,95 | `795f49c7cbc34777ba40e7cd8c57894ac92132ebf6f5971fafac63a1c88c61d0` |
| [`pkg/schedule/labeler/keyspace_index_test.go`](../../../pkg/schedule/labeler/keyspace_index_test.go) | 70,95 | `29267672bcc9e4824ae748fb0e770bb4be58f5797ce6354155d5b18e9d16f165` |
| [`pkg/schedule/labeler/label_gc_test.rs`](../../../pkg/schedule/labeler/label_gc_test.rs) | 70,95 | `377f9d4c74b7fd784e0e9c608d3988097cea77214e585fb215b4d1ee6a1590b1` |
| [`pkg/schedule/labeler/label_index_test.rs`](../../../pkg/schedule/labeler/label_index_test.rs) | 70,95 | `a0a395ff556a9a714b8e062574db9152066117b4bcc6f9a709fdb6eb76fe16f6` |
| [`pkg/schedule/labeler/label_keyspace_test.rs`](../../../pkg/schedule/labeler/label_keyspace_test.rs) | 70,95 | `a83694d3901a8dfee8e8573c20ea9be9d49be35406b85e17f40ddf0dadb52283` |
| [`pkg/schedule/labeler/label_query.rs`](../../../pkg/schedule/labeler/label_query.rs) | 70,95 | `a19cc20b5ecc1e20e82fe6ff8778ca275928f9f8f5aaa6e638d3ce6b85f835f1` |
| [`pkg/schedule/labeler/label_query.rs.md`](../../../pkg/schedule/labeler/label_query.rs.md) | 70,95 | `17e093800b863159f9de30a2eb7f594b210e0d47a16b7b44b51cbac8b541697f` |
| [`pkg/schedule/labeler/label_query_test.rs`](../../../pkg/schedule/labeler/label_query_test.rs) | 70,95 | `99f409e1ec992b63cdcd13798f227f3d1d4be61729fa506e0cc2b38b39a2dda6` |
| [`pkg/schedule/labeler/label_restore_test.rs`](../../../pkg/schedule/labeler/label_restore_test.rs) | 70,95 | `ffe4a7295726500c47fc4fb2091c2821b1efa0c2f0997d71859c816cec9233ad` |
| [`pkg/schedule/labeler/label_rule_test.rs`](../../../pkg/schedule/labeler/label_rule_test.rs) | 70,95 | `9ca22a5f97b57467996dd9c23b7bc5f7155cfb000cb274657bd1bdacd2544f4b` |
| [`pkg/schedule/labeler/label_txn_test.rs`](../../../pkg/schedule/labeler/label_txn_test.rs) | 70,95 | `9427021ed17cb08fc2988576e41e6e3ef839631e94c1b08bf70098bd79287fd5` |
| [`pkg/schedule/labeler/labeler.go`](../../../pkg/schedule/labeler/labeler.go) | 70,95 | `d05b46f49fe1eaa5c3cd6bff2d06c3c17ddcda13b15d064f34edaccf133a54fc` |
| [`pkg/schedule/labeler/labeler.rs`](../../../pkg/schedule/labeler/labeler.rs) | 70,95 | `51bcddab28822068f3bf59d3829294dbd37ce8c6accfb071d6a9fa67aca8f808` |
| [`pkg/schedule/labeler/labeler.rs.md`](../../../pkg/schedule/labeler/labeler.rs.md) | 70,95 | `eb2888f4b77afae396d0a94677569a84042dea01372215a299c344441e51901d` |
| [`pkg/schedule/labeler/labeler_test.go`](../../../pkg/schedule/labeler/labeler_test.go) | 70,95 | `c66e25c0f55a0136e7472e8757495391b866d382d514cb0ec2bb8694fc107a5e` |
| [`pkg/schedule/labeler/lib.rs`](../../../pkg/schedule/labeler/lib.rs) | 70,95 | `050daa4cfbcecf0e2f1e5bee6a6927c23c9452d849484d934812e3d827682fdb` |
| [`pkg/schedule/labeler/metrics.go`](../../../pkg/schedule/labeler/metrics.go) | 70,95 | `86ee9bbf981a6eac4a17f36e3cd0e3711d4e928d2c6a9b42525d8f65f5c43f4b` |
| [`pkg/schedule/labeler/plan.go`](../../../pkg/schedule/labeler/plan.go) | 70,95 | `bfb2785336157e489782d913ef6e28416b77f10c719d22ddf77e065eac8ff65a` |
| [`pkg/schedule/labeler/plan.rs`](../../../pkg/schedule/labeler/plan.rs) | 70,95 | `5a89e59ecf8c1acf095f32d9fbd7c79b49c95597b25b25ad726a56e28747cca1` |
| [`pkg/schedule/labeler/plan.rs.md`](../../../pkg/schedule/labeler/plan.rs.md) | 70,95 | `0f782e49757077861345df2f4a52d3000468137c07eb8025b20f166d4af03f4d` |
| [`pkg/schedule/labeler/plan_test.go`](../../../pkg/schedule/labeler/plan_test.go) | 70,95 | `8f4bd74c9223d0961e970265bdfd291c048959438e88213040c22e6eb2907c18` |
| [`pkg/schedule/labeler/rule_index.go`](../../../pkg/schedule/labeler/rule_index.go) | 70,95 | `0a142100a9293d2944c446f13b740bf264f17a282892adc5d4a0c29d4805d15f` |
| [`pkg/schedule/labeler/rule_index.rs`](../../../pkg/schedule/labeler/rule_index.rs) | 70,95 | `a7c86ce5331bf5b4b5e2285ae6fefc18eb132ba11109b68f9b0224d0456a8d2d` |
| [`pkg/schedule/labeler/rule_index.rs.md`](../../../pkg/schedule/labeler/rule_index.rs.md) | 70,95 | `2af785b7793e88f593fc4e13715950a93cefeedbda329404f537b80f66a2c92b` |
| [`pkg/schedule/labeler/rule_test.go`](../../../pkg/schedule/labeler/rule_test.go) | 70,95 | `be7db74c7bb194325898f0d3749a664843e801585cdf89d8cecbf9202a590cf3` |
| [`pkg/schedule/labeler/rules.go`](../../../pkg/schedule/labeler/rules.go) | 70,95 | `74732410d32f922488ab1d1542d9e82ea421ed62c878c83ad70a60e8b3c6b30d` |
| [`pkg/schedule/labeler/rules.rs`](../../../pkg/schedule/labeler/rules.rs) | 70,95 | `ec661bf6602e94f547c1f20ea6b0319b1b0e52a8d65c2368d5415c5460d6c6e1` |
| [`pkg/schedule/labeler/rules.rs.md`](../../../pkg/schedule/labeler/rules.rs.md) | 70,95 | `800a17774caf7f6cd0bc6170d78abd6462127818eaa2a7ce2caa976c44d4d608` |
| [`pkg/schedule/lib.rs`](../../../pkg/schedule/lib.rs) | 95 | `b329fde42510bc1498e5aa56ca6859155ef9310ad1b68ce6719824159c9252af` |
| [`pkg/schedule/metrics.go`](../../../pkg/schedule/metrics.go) | 95 | `a5d4ae0dbc395de732e1d307f0ccecb54319561ccb8d77ee4684fcf23e5c5791` |
| [`pkg/schedule/operator/Cargo.toml`](../../../pkg/schedule/operator/Cargo.toml) | 62,95 | `3af69dccf8e49d1a91b2845dd5abe5bb1293b473096b7178d1977803586ef27a` |
| [`pkg/schedule/operator/apply_steps.rs`](../../../pkg/schedule/operator/apply_steps.rs) | 62,95 | `a8254acb81a1240ba6554214d95103e309700e8e9a2d29fa7fecb1a1c557a825` |
| [`pkg/schedule/operator/apply_steps.rs.md`](../../../pkg/schedule/operator/apply_steps.rs.md) | 62,95 | `47271c64d2c75808d76e77d80ca5b33a8e4fabe4df5e227316ad85c2414bb3d6` |
| [`pkg/schedule/operator/builder.go`](../../../pkg/schedule/operator/builder.go) | 62,95 | `2c74830947c401ea8c4d46e8066e4c85aecd02beea035d0bb9eb4aab8cee8f83` |
| [`pkg/schedule/operator/builder_joint.rs`](../../../pkg/schedule/operator/builder_joint.rs) | 62,95 | `fcb70f40ce51fcbeb641ee38f1d3e5ee4d4020b49f3635cf8c373f2ffc27b3ef` |
| [`pkg/schedule/operator/builder_joint.rs.md`](../../../pkg/schedule/operator/builder_joint.rs.md) | 62,95 | `fe9d8710fb80fb3a1f24d83ee58b1face1ba72ff9207f159131b10ce39a3bc05` |
| [`pkg/schedule/operator/builder_leader.rs`](../../../pkg/schedule/operator/builder_leader.rs) | 62,95 | `49ab2bc2b11012cdf673afece4745d4ee239892fc684887d3b812356a9d63760` |
| [`pkg/schedule/operator/builder_leader.rs.md`](../../../pkg/schedule/operator/builder_leader.rs.md) | 62,95 | `56fa2c0fea2a14abff7d6065d331ef7812ca383166b3443d6152428dce397301` |
| [`pkg/schedule/operator/builder_simple.rs`](../../../pkg/schedule/operator/builder_simple.rs) | 62,95 | `9cf1a27a5d80dfd5f15faabb556c178330d52a9099331282b8ba294a97126c40` |
| [`pkg/schedule/operator/builder_simple.rs.md`](../../../pkg/schedule/operator/builder_simple.rs.md) | 62,95 | `93af52e9c77d0d1110e7d7caf299932d3778b6e60419bd1acbfd77e38471a628` |
| [`pkg/schedule/operator/builder_target.rs`](../../../pkg/schedule/operator/builder_target.rs) | 62,95 | `060c6c4306b70dd9e8ee304ac1e88247ddebd9fd96074190875d384fff501ed1` |
| [`pkg/schedule/operator/builder_target.rs.md`](../../../pkg/schedule/operator/builder_target.rs.md) | 62,95 | `0e60be9cfe8f4007cc2960b0eaf24b5d43ddc96e24c60177cf2a13b8c6f867d9` |
| [`pkg/schedule/operator/builder_test.go`](../../../pkg/schedule/operator/builder_test.go) | 62,95 | `3c3e8b4681b988523e129dd3a0452e9f03c14273b118666984268432dbb4c953` |
| [`pkg/schedule/operator/builder_witness.rs`](../../../pkg/schedule/operator/builder_witness.rs) | 62,95 | `c26b788efa09041cfa401c6860425e605b7344f8d9d6b131e929c55c851c6478` |
| [`pkg/schedule/operator/builder_witness.rs.md`](../../../pkg/schedule/operator/builder_witness.rs.md) | 62,95 | `b924694ce6c3876fcb98d7de8aab58feadfb333399750a05d856b18276959b4b` |
| [`pkg/schedule/operator/controller_admission.rs`](../../../pkg/schedule/operator/controller_admission.rs) | 62,95 | `48ff7a94e46c61261bf77a4f917258301c05dde0f7714420b0604b56cf98ec2c` |
| [`pkg/schedule/operator/controller_admission.rs.md`](../../../pkg/schedule/operator/controller_admission.rs.md) | 62,95 | `40926dfa7af38323db85f5248c766735d69adb7c7055eed0e8dce9c97694234d` |
| [`pkg/schedule/operator/controller_dispatch.rs`](../../../pkg/schedule/operator/controller_dispatch.rs) | 62,95 | `3820b7657012d3986167fdd83cdc3c0b9dfbe538251d8cdadf6c64daf88828e2` |
| [`pkg/schedule/operator/controller_dispatch.rs.md`](../../../pkg/schedule/operator/controller_dispatch.rs.md) | 62,95 | `008bdfed315225515bf221bea338f01ad6bb21585700c51ffc0599ccda60a86d` |
| [`pkg/schedule/operator/controller_limit.rs`](../../../pkg/schedule/operator/controller_limit.rs) | 62,95 | `45212f207eb8f9190996a42106973aa3404fe0f653b78aa31df37b6ba9a6e25d` |
| [`pkg/schedule/operator/controller_limit.rs.md`](../../../pkg/schedule/operator/controller_limit.rs.md) | 62,95 | `acbcf07420636cd44338affa3950526b1943f6e96ed17bdc27c3c8e7af207836` |
| [`pkg/schedule/operator/controller_records.rs`](../../../pkg/schedule/operator/controller_records.rs) | 62,95 | `a4f2ab3194efc16a6cd6319eb64a29532364dcffda2b9c43d021dea941fba805` |
| [`pkg/schedule/operator/controller_records.rs.md`](../../../pkg/schedule/operator/controller_records.rs.md) | 62,95 | `9d8727e1f04a36063100da6d6ca8487e2304756f8391f8ab7c0a51e3938bdb53` |
| [`pkg/schedule/operator/create_operator.go`](../../../pkg/schedule/operator/create_operator.go) | 62,95 | `43a938c72992f4ce81501c40ab85c1e79c08670383acd47f22655e779524b927` |
| [`pkg/schedule/operator/create_operator_test.go`](../../../pkg/schedule/operator/create_operator_test.go) | 62,95 | `919a902cd88c9ced64be15c154b409dccd56de584266f6f41137caa963bfbb3c` |
| [`pkg/schedule/operator/create_peer.rs`](../../../pkg/schedule/operator/create_peer.rs) | 62,95 | `876af4bbad40f1dcddddb154fb4d11319c98e9e23ca69d4ecaa6e404849b0956` |
| [`pkg/schedule/operator/create_peer.rs.md`](../../../pkg/schedule/operator/create_peer.rs.md) | 62,95 | `1c24b8757ccc2b5b4cedf9f64206c4c321c9fd41b37e9edb02636b383cfaa00d` |
| [`pkg/schedule/operator/create_range.rs`](../../../pkg/schedule/operator/create_range.rs) | 62,95 | `ce339d90ecfa3f78bc2fd899b31ba07c22b61f1a57b6961486753bb4c3d7bb5b` |
| [`pkg/schedule/operator/create_range.rs.md`](../../../pkg/schedule/operator/create_range.rs.md) | 62,95 | `ebe0178464aff91871fdc13f1d35ef60056451382117e8d9819a3cd2027f0e78` |
| [`pkg/schedule/operator/influence.go`](../../../pkg/schedule/operator/influence.go) | 62,95 | `28181ccd446b1309976e1aa2d55c56ab6bfec2f3dd57dd5d0b724c9fc0605622` |
| [`pkg/schedule/operator/influence.rs`](../../../pkg/schedule/operator/influence.rs) | 62,95 | `f3ae622617e9137636ff985b006c651fb2146fa04fabce89ccb2a7f304c219cc` |
| [`pkg/schedule/operator/influence.rs.md`](../../../pkg/schedule/operator/influence.rs.md) | 62,95 | `8daf6b2ad34e9c2a17c62d6984da3f47f9e849ca8986fea0cd92a5204d8284b2` |
| [`pkg/schedule/operator/kind.go`](../../../pkg/schedule/operator/kind.go) | 62,95 | `b9fad78bd44d1f0e2077bc8d22dc875aedb1ab8fb6a160086dd2a166c217a133` |
| [`pkg/schedule/operator/lib.rs`](../../../pkg/schedule/operator/lib.rs) | 62,95 | `6ae205f7d69f41bf9a97d8be463f2b06469b7d55be94be39801c6ebee44413d4` |
| [`pkg/schedule/operator/metrics.go`](../../../pkg/schedule/operator/metrics.go) | 62,95 | `59d651a15bbc27f6f8911291afa78e42f4e89e84c33da6bb3e78a22f874edf8a` |
| [`pkg/schedule/operator/operator.go`](../../../pkg/schedule/operator/operator.go) | 62,95 | `ad287d89ee51f9d8d94985b45f11b2c4baddae0acb59c2f7899bc607fca860ba` |
| [`pkg/schedule/operator/operator.rs`](../../../pkg/schedule/operator/operator.rs) | 62,95 | `10fde69d96844a83b2ea79e83376cda978dc643d537b2ac284cd4bd3d1a83de8` |
| [`pkg/schedule/operator/operator.rs.md`](../../../pkg/schedule/operator/operator.rs.md) | 62,95 | `51db34a3e19981a2389878d3d59559eeae49263a9286c11c6bbddeb12155de82` |
| [`pkg/schedule/operator/operator_controller.go`](../../../pkg/schedule/operator/operator_controller.go) | 62,95 | `173d8e3e5fd5e4b1128ba24a954055d2d1ef7ea7a6676f6d3aa7cc7a37672ba1` |
| [`pkg/schedule/operator/operator_controller_test.go`](../../../pkg/schedule/operator/operator_controller_test.go) | 62,95 | `18e953a9c41e84c3dba9dc381ebce462f652b27d0f7ab83de22413bc84e21501` |
| [`pkg/schedule/operator/operator_queue.go`](../../../pkg/schedule/operator/operator_queue.go) | 62,95 | `ba2e0fcfb8823c3b428194cbb7083045d9b65019e88ccfe08f905762763b40fc` |
| [`pkg/schedule/operator/operator_record.rs`](../../../pkg/schedule/operator/operator_record.rs) | 62,95 | `531b8f0873e295ade9c705f48f5c0b8602f518c7b4107f5d5706d3d2a093d637` |
| [`pkg/schedule/operator/operator_record.rs.md`](../../../pkg/schedule/operator/operator_record.rs.md) | 62,95 | `5d62121c63d873a9b4e8c990a14b6a1b789a0af0e56328701b2a291d6b297423` |
| [`pkg/schedule/operator/operator_test.go`](../../../pkg/schedule/operator/operator_test.go) | 62,95 | `96bc7513e10013b3495b42b3aeb4425f4e04f415712a472fe1a457e0d08f71e9` |
| [`pkg/schedule/operator/queues.rs`](../../../pkg/schedule/operator/queues.rs) | 62,95 | `0105906f259729c35f8d7655dbc2d5180a7153618f4cf67c7bde3f2b17a445b3` |
| [`pkg/schedule/operator/queues.rs.md`](../../../pkg/schedule/operator/queues.rs.md) | 62,95 | `6f0b6da62821d4bdc94b5038dcf5c68fc6b39c704780d2849aa059868ad74d4e` |
| [`pkg/schedule/operator/status.go`](../../../pkg/schedule/operator/status.go) | 62,95 | `74455d16a6a9ff42cb97ac7791edf2ef46077013a7959d6da82259b8f8942090` |
| [`pkg/schedule/operator/status.rs`](../../../pkg/schedule/operator/status.rs) | 62,95 | `5cfed21c75f2871d14ee217028cbf36e5175b9997a723b9e01dce869445da126` |
| [`pkg/schedule/operator/status.rs.md`](../../../pkg/schedule/operator/status.rs.md) | 62,95 | `b7a0c05e92841850eda5c7c5d2d0f3fab7a3d5c3824dbde664dc85f72bacab9b` |
| [`pkg/schedule/operator/status_test.go`](../../../pkg/schedule/operator/status_test.go) | 62,95 | `ec3755ceff6e47e7feaa66bd91d357d30ac70caeae29334f1855555acfaa2756` |
| [`pkg/schedule/operator/status_tracker.go`](../../../pkg/schedule/operator/status_tracker.go) | 62,95 | `7d573156bbd05679f4929b26081983b450ffa190494826da2f7ff1fcbb342ffb` |
| [`pkg/schedule/operator/status_tracker_test.go`](../../../pkg/schedule/operator/status_tracker_test.go) | 62,95 | `a627197d6a59fb19eaf8f7edd53c94832ea9dc4d633874dd644b9f8649e9af1b` |
| [`pkg/schedule/operator/step.go`](../../../pkg/schedule/operator/step.go) | 62,95 | `9338b311dca1940cd76bbe3903f0d2545cfc258f88a20eedb391ab59e243eac5` |
| [`pkg/schedule/operator/step_add.rs`](../../../pkg/schedule/operator/step_add.rs) | 62,95 | `36f1fe99cb452ddf791d32b9f5cad047778099eb8ef45686c86b1a6e40aabee9` |
| [`pkg/schedule/operator/step_add.rs.md`](../../../pkg/schedule/operator/step_add.rs.md) | 62,95 | `805bcde32d62190e75faec994a9772fb4d21daaf301f2de611082bebc65fa300` |
| [`pkg/schedule/operator/step_joint.rs`](../../../pkg/schedule/operator/step_joint.rs) | 62,95 | `fbb9b4648a1cf1e9c73cab1d5859298e6fa9cca9c770661581740fdd00bee4b1` |
| [`pkg/schedule/operator/step_joint.rs.md`](../../../pkg/schedule/operator/step_joint.rs.md) | 62,95 | `649d59e6b4f9a4e64df50fbb6125a5f013f9f885baa89bcaeee9218bbfd86894` |
| [`pkg/schedule/operator/step_leader.rs`](../../../pkg/schedule/operator/step_leader.rs) | 62,95 | `57d6382d749048dd742eb336313f9546ab8320e101b6c5e551ef9109af0bdea2` |
| [`pkg/schedule/operator/step_leader.rs.md`](../../../pkg/schedule/operator/step_leader.rs.md) | 62,95 | `af471bd029159e3d6056a266a2b2f25a935444b7ddc5d7ca4c21f22e7f51ead2` |
| [`pkg/schedule/operator/step_range.rs`](../../../pkg/schedule/operator/step_range.rs) | 62,95 | `a431a6730c25e1bf1a0ee9fefc8886fec69d3ab1017efedf5c48b856615231d8` |
| [`pkg/schedule/operator/step_range.rs.md`](../../../pkg/schedule/operator/step_range.rs.md) | 62,95 | `68af559e202af568bd64db86251e59e62d48a85a003f13a3e42c6ca83942226c` |
| [`pkg/schedule/operator/step_remove.rs`](../../../pkg/schedule/operator/step_remove.rs) | 62,95 | `34d71e2478d385a8036476d9243c4557a82e7e335d6172e4c0421a122f23ac55` |
| [`pkg/schedule/operator/step_remove.rs.md`](../../../pkg/schedule/operator/step_remove.rs.md) | 62,95 | `42b6f393431ef32a37d0528199b2e0d0c9c793f051215ba5a5c46b5adc3c864d` |
| [`pkg/schedule/operator/step_test.go`](../../../pkg/schedule/operator/step_test.go) | 62,95 | `cc353a5b70022385630b93ce77f8a8e637c4a51c7724dc9c29ae3ef696c56c7b` |
| [`pkg/schedule/operator/step_witness.rs`](../../../pkg/schedule/operator/step_witness.rs) | 62,95 | `9e5b1daded5f95096186576874e852dcfee790922b63003d9685e819ee0fa9ab` |
| [`pkg/schedule/operator/step_witness.rs.md`](../../../pkg/schedule/operator/step_witness.rs.md) | 62,95 | `4408a7d79575ca8b40c5d52f8e87bb8ea634e3e880fa48ed9fe6f10452d4b74a` |
| [`pkg/schedule/operator/task_015_test.rs`](../../../pkg/schedule/operator/task_015_test.rs) | 62,95 | `f006f6ae5155d12c9a7b900df0db9968836d0c76f796c48a56ad03cc1e4937b8` |
| [`pkg/schedule/operator/task_016_test.rs`](../../../pkg/schedule/operator/task_016_test.rs) | 62,95 | `b96f5b53fb57636b86a1f998e52b03c20477cd9f9d403dd8b1a90c8d8f145de1` |
| [`pkg/schedule/operator/task_017_test.rs`](../../../pkg/schedule/operator/task_017_test.rs) | 62,95 | `42ca2bf96a3d90f7493a538d3bbe856f755374abeb23f2d8c8e624f69178d921` |
| [`pkg/schedule/operator/task_018_test.rs`](../../../pkg/schedule/operator/task_018_test.rs) | 62,95 | `6672d7d8294a48aa13cf09da7c5818a69cad6bd210b47ccadb6053f9e49f3f81` |
| [`pkg/schedule/operator/task_019_test.rs`](../../../pkg/schedule/operator/task_019_test.rs) | 62,95 | `af10436eea928abf6629a1ff27635a510f223e4660355e7928b50315554a3116` |
| [`pkg/schedule/operator/task_020_test.rs`](../../../pkg/schedule/operator/task_020_test.rs) | 62,95 | `d9c789e3ec588a89e1a5108aab685d424e0bd04be347d29e2b5d46fe1697f599` |
| [`pkg/schedule/operator/task_021_test.rs`](../../../pkg/schedule/operator/task_021_test.rs) | 62,95 | `e74ae305f157efe9a7ba98e57838c2139935d78effb0b5a37949e6e4a49d19ba` |
| [`pkg/schedule/operator/task_022_test.rs`](../../../pkg/schedule/operator/task_022_test.rs) | 62,95 | `29291f626891f23371197780d72c4af97443df24b350ac9224dac791971278cd` |
| [`pkg/schedule/operator/task_023_test.rs`](../../../pkg/schedule/operator/task_023_test.rs) | 62,95 | `2c8cdb30c1eef7d1b2ba0c4f69725175b758c935d3f5602113ef594ddb3af9b9` |
| [`pkg/schedule/operator/task_024_test.rs`](../../../pkg/schedule/operator/task_024_test.rs) | 62,95 | `eca404eea2d9ead4d1db937e58ce401739598231bba3b35ab9a22babb1449d7a` |
| [`pkg/schedule/operator/task_025_test.rs`](../../../pkg/schedule/operator/task_025_test.rs) | 62,95 | `d46ecce58f8fcdb34481fe0bda41b08b909c4a544a28664140c3e98916b5f603` |
| [`pkg/schedule/operator/task_026_test.rs`](../../../pkg/schedule/operator/task_026_test.rs) | 62,95 | `83f25cb44bc63dfa9d42710c8b45f9d3698fbc888c58e581e4cbb2d7952c1339` |
| [`pkg/schedule/operator/task_027_test.rs`](../../../pkg/schedule/operator/task_027_test.rs) | 62,95 | `c95d06fa05794a65f8770b687aa3b880fb24f9f93a8f039be23e471fc2157df6` |
| [`pkg/schedule/operator/task_028_test.rs`](../../../pkg/schedule/operator/task_028_test.rs) | 62,95 | `a64c9f96c6664e6679d8630d2c2b9733e29b8f5a9c67ff503668f779043f1c2e` |
| [`pkg/schedule/operator/task_029_test.rs`](../../../pkg/schedule/operator/task_029_test.rs) | 62,95 | `e0fbc8162a9e9f6e4dd7bfbd2927781b9ab461a661aea452d349fca43f217d09` |
| [`pkg/schedule/operator/task_030_test.rs`](../../../pkg/schedule/operator/task_030_test.rs) | 62,95 | `b64b5d8f3a1739a4b7ab4935233bfc20ac6487717c4a920678a336011cba3fe3` |
| [`pkg/schedule/operator/task_031_test.rs`](../../../pkg/schedule/operator/task_031_test.rs) | 62,95 | `6fcff1945fe7fd1d51e07f811b7ca2930354908d9a442088bc483b5d35615cc1` |
| [`pkg/schedule/operator/task_032_test.rs`](../../../pkg/schedule/operator/task_032_test.rs) | 62,95 | `67c2c245d01bf22f44c1fbbb0bca6485c127712b525431f71b843a21c0e05322` |
| [`pkg/schedule/operator/task_033_test.rs`](../../../pkg/schedule/operator/task_033_test.rs) | 62,95 | `aac7dced2053ad3e0181352dabfcf581df46ee17452806a9cf185391feaa281f` |
| [`pkg/schedule/operator/task_034_test.rs`](../../../pkg/schedule/operator/task_034_test.rs) | 62,95 | `750829217243a0b33dd20ba6ec8f7fbc36429994dcd7d28f76f43382654dbf71` |
| [`pkg/schedule/operator/task_035_test.rs`](../../../pkg/schedule/operator/task_035_test.rs) | 62,95 | `af2dec2ed30f9e2f29bcbdb6c6a98d36ed7ff1f5502a5cc5f14574911699bad0` |
| [`pkg/schedule/operator/task_036_test.rs`](../../../pkg/schedule/operator/task_036_test.rs) | 62,95 | `828b809b7c378b90592ca9ad3a81eabead7975abd8690fc120bc0cddbbf2d86b` |
| [`pkg/schedule/operator/task_037_test.rs`](../../../pkg/schedule/operator/task_037_test.rs) | 62,95 | `2fc065887d6b3db310a691b59af7dd8a008b104a94b6eab95486ab8046b8f8c0` |
| [`pkg/schedule/operator/task_038_test.rs`](../../../pkg/schedule/operator/task_038_test.rs) | 62,95 | `b415faeec4e8941478c3a34557c12c1ed5529e999da2d39f56d9800c20d9fb90` |
| [`pkg/schedule/operator/test_util.go`](../../../pkg/schedule/operator/test_util.go) | 62,95 | `ddce7bfda595016a6c8fcb9ca655e5d02a6773b74b06b7821b10723b6e791395` |
| [`pkg/schedule/operator/waiting_operator.go`](../../../pkg/schedule/operator/waiting_operator.go) | 62,95 | `cb4673dec59b3d24517ba02c0a5ff07b08a1409be13a289eb4bbc7427a53f32e` |
| [`pkg/schedule/operator/waiting_operator_test.go`](../../../pkg/schedule/operator/waiting_operator_test.go) | 62,95 | `bed14db27053fe79684827b05c264e12708bf31d846301a5bb6973f9df0941cf` |
| [`pkg/schedule/placement/Cargo.toml`](../../../pkg/schedule/placement/Cargo.toml) | 68,69,95 | `211c9cc8165913a307360b5d0ab40bb54c9151a0d23ac058f40e52dc66182c6d` |
| [`pkg/schedule/placement/config.go`](../../../pkg/schedule/placement/config.go) | 68,69,95 | `0c090b61ddd1716ec7f85447ee935b306cc1306e0d47f307fb341524a5298008` |
| [`pkg/schedule/placement/config.rs`](../../../pkg/schedule/placement/config.rs) | 68,69,95 | `42f9e3474d9386dbcbcb490b2ff8d1afe3d6c807ea8db7553efa9cef3c34ae06` |
| [`pkg/schedule/placement/config.rs.md`](../../../pkg/schedule/placement/config.rs.md) | 68,69,95 | `b9717923a42630e3b3300c100fce5e5d025d7b61326ad99edbd0b55963a41028` |
| [`pkg/schedule/placement/config_test.go`](../../../pkg/schedule/placement/config_test.go) | 68,69,95 | `1723ae879bb596224843e4041f830977c4175cf17fc09a550e0a0a3e2eb1aea2` |
| [`pkg/schedule/placement/fit.go`](../../../pkg/schedule/placement/fit.go) | 68,69,95 | `5e5c91601641ee4ffd1af5a7fedd3424bf5408d210fdb03b3956f52bb5da7b22` |
| [`pkg/schedule/placement/fit.rs`](../../../pkg/schedule/placement/fit.rs) | 68,69,95 | `449594b04931b9e012960b0b094d6f751239029dfb5a204fed0614e502dacf61` |
| [`pkg/schedule/placement/fit.rs.md`](../../../pkg/schedule/placement/fit.rs.md) | 68,69,95 | `aa315625e525bc68d97332f0c661d26f625a25c8771a72b1eea84785556975f7` |
| [`pkg/schedule/placement/fit_isolation.rs`](../../../pkg/schedule/placement/fit_isolation.rs) | 68,69,95 | `7df9d4e08ae0eb8176273ea141472d3533eaa8726948353206cbf202b0d31e05` |
| [`pkg/schedule/placement/fit_isolation.rs.md`](../../../pkg/schedule/placement/fit_isolation.rs.md) | 68,69,95 | `8a17b8b26ca033ca51833c6d8e5802f1a193b43246c49a50bf3f460b5f85a1c9` |
| [`pkg/schedule/placement/fit_region_test.go`](../../../pkg/schedule/placement/fit_region_test.go) | 68,69,95 | `b04a331bad4f9ea28c55ad8a6fbbfc86e4d39be2de8f3ff7269ad3bc79e7449b` |
| [`pkg/schedule/placement/fit_test.go`](../../../pkg/schedule/placement/fit_test.go) | 68,69,95 | `d69772327eb5bb4d0f3e934486323bcfe4e20ff524e2eff230bb42ccaa20b724` |
| [`pkg/schedule/placement/json.rs`](../../../pkg/schedule/placement/json.rs) | 68,69,95 | `16b0ed551369fd03d91c436e3e8ac7391422166850b4cfc89d6c11faed84098c` |
| [`pkg/schedule/placement/json.rs.md`](../../../pkg/schedule/placement/json.rs.md) | 68,69,95 | `58338b72ec593d68fe9d0da1a16141cd7cda36a330f52009fb114536d172a2d2` |
| [`pkg/schedule/placement/label_constraint.go`](../../../pkg/schedule/placement/label_constraint.go) | 68,69,95 | `335872cd9d2add785207e4d3d8e5cfe4a64d1ef8badc847fed904da797db7982` |
| [`pkg/schedule/placement/label_constraint.rs`](../../../pkg/schedule/placement/label_constraint.rs) | 68,69,95 | `e77985188410cc49ba19a31462e89136606542e5504302a7a46c4913102fab55` |
| [`pkg/schedule/placement/label_constraint.rs.md`](../../../pkg/schedule/placement/label_constraint.rs.md) | 68,69,95 | `8f9235134cb833e92202275d6affc21956559ff2258adcce8ab555d7a537d353` |
| [`pkg/schedule/placement/label_constraint_test.go`](../../../pkg/schedule/placement/label_constraint_test.go) | 68,69,95 | `d50949d627cc44620d84b6e10d1fab5f96d36b8e86259faca4fba1f7d4c33398` |
| [`pkg/schedule/placement/lib.rs`](../../../pkg/schedule/placement/lib.rs) | 68,69,95 | `14af7bfba0fb64950a9979abbb07ba2fca92a88a91abd99370dda9077db0e426` |
| [`pkg/schedule/placement/lib.rs.md`](../../../pkg/schedule/placement/lib.rs.md) | 68,69,95 | `00e000b3723a6413c6212eee641a300d759cd2ac9e77194949cf654b4c44c860` |
| [`pkg/schedule/placement/placement_batch_test.rs`](../../../pkg/schedule/placement/placement_batch_test.rs) | 68,69,95 | `ce8037017c6516469f8458fbceea79b48a0e88e558d96e0e945ba222902e034a` |
| [`pkg/schedule/placement/placement_cache_test.rs`](../../../pkg/schedule/placement/placement_cache_test.rs) | 68,69,95 | `f2d56d3e95dfc52862e32ec30d81765a0c7202f4ef36270f2b0e3597d14e3c35` |
| [`pkg/schedule/placement/placement_isolation_test.rs`](../../../pkg/schedule/placement/placement_isolation_test.rs) | 68,69,95 | `d9fc5d99fe7458918f6ce8e0a8e1d772d73725bcade305b1e6d69abc3aefd7a1` |
| [`pkg/schedule/placement/placement_manager_test.rs`](../../../pkg/schedule/placement/placement_manager_test.rs) | 68,69,95 | `90dd4e34974c04c28b6862a3a52e2a0b9ba4912d5392eaa72c7937cf31f9aa04` |
| [`pkg/schedule/placement/placement_order_test.rs`](../../../pkg/schedule/placement/placement_order_test.rs) | 68,69,95 | `cf944c482c601dc639483f09bf52bf1f8c359c8306101cb64887e2608638561f` |
| [`pkg/schedule/placement/placement_patch_test.rs`](../../../pkg/schedule/placement/placement_patch_test.rs) | 68,69,95 | `f765e11b505ce2dbad85ca4d982b0296a886e908c100c8d823f95f405980c5b8` |
| [`pkg/schedule/placement/placement_region_fit_test.rs`](../../../pkg/schedule/placement/placement_region_fit_test.rs) | 68,69,95 | `b710f2511531349ed9f56079b1e3c2a24edd12addaff0c57b39ccc635ebd56fc` |
| [`pkg/schedule/placement/placement_restore_test.rs`](../../../pkg/schedule/placement/placement_restore_test.rs) | 68,69,95 | `90df77a32f1380b0ff41fb5fc8a32c7bb082deb6e67117e8efd88dcef0ccb5d8` |
| [`pkg/schedule/placement/placement_role_fit_test.rs`](../../../pkg/schedule/placement/placement_role_fit_test.rs) | 68,69,95 | `82ceb15f82ce29eb2a9ddde061b1ef96ddc9b3f187f1b39ded8e80651cc64923` |
| [`pkg/schedule/placement/placement_values_test.rs`](../../../pkg/schedule/placement/placement_values_test.rs) | 68,69,95 | `32dfb055268a58fe3ed1af8600d3f4b745128a678c4842a60bcdb5b04885cf5b` |
| [`pkg/schedule/placement/region_rule_cache.go`](../../../pkg/schedule/placement/region_rule_cache.go) | 68,69,95 | `3aed4ea52a964b4ccf5d59d75b54478aa480a7ae4991143b10d9817c59e1b190` |
| [`pkg/schedule/placement/region_rule_cache.rs`](../../../pkg/schedule/placement/region_rule_cache.rs) | 68,69,95 | `990d4a945a4e0ae349656832fab69c621c2e029f852ceb84193a10c78f871757` |
| [`pkg/schedule/placement/region_rule_cache.rs.md`](../../../pkg/schedule/placement/region_rule_cache.rs.md) | 68,69,95 | `b556ac36858b1ec2ec53037915b71b7ab33413d39b55c9bec3a06681d36bffa5` |
| [`pkg/schedule/placement/region_rule_cache_test.go`](../../../pkg/schedule/placement/region_rule_cache_test.go) | 68,69,95 | `c843c34fc2a852cffc806b74805fb0e36dd82e3dc8426aee95ab9b655a06752f` |
| [`pkg/schedule/placement/rule.go`](../../../pkg/schedule/placement/rule.go) | 68,69,95 | `283a068a2c5b52adb0df94e02c4b29fa20252967eb9158373897b491091863e1` |
| [`pkg/schedule/placement/rule.rs`](../../../pkg/schedule/placement/rule.rs) | 68,69,95 | `8e1338db35d410f61f983738abc495a61fd21ab522d0dd7de35d6b22b49fdaa2` |
| [`pkg/schedule/placement/rule.rs.md`](../../../pkg/schedule/placement/rule.rs.md) | 68,69,95 | `5e42d1ab5c82a1c1cd0181111aa775759f9cd4e901fc796ada9c644e8b4c1880` |
| [`pkg/schedule/placement/rule_batch.rs`](../../../pkg/schedule/placement/rule_batch.rs) | 68,69,95 | `e8a55b6abe199a0c7050bbef98981624e1b2d8b9acfd125a308e663f7b2ae0d6` |
| [`pkg/schedule/placement/rule_batch.rs.md`](../../../pkg/schedule/placement/rule_batch.rs.md) | 68,69,95 | `babd1b71dd994af1d8c2819464b1ab6f6597404065fc1afa385bf059056ac45f` |
| [`pkg/schedule/placement/rule_list.go`](../../../pkg/schedule/placement/rule_list.go) | 68,69,95 | `f957de33b33f0fbb9484821a00fd6d58ac5eb572263c2fc9db7a0c95b4467680` |
| [`pkg/schedule/placement/rule_list.rs`](../../../pkg/schedule/placement/rule_list.rs) | 68,69,95 | `ec3000597acee933d808b7448dd44889d4fc5b960502c39ff4cd96e46787ecdd` |
| [`pkg/schedule/placement/rule_list.rs.md`](../../../pkg/schedule/placement/rule_list.rs.md) | 68,69,95 | `d543a2073d8cae34c5827adfef86910d2e71aafcc964df17c5e451370e1a6754` |
| [`pkg/schedule/placement/rule_manager.go`](../../../pkg/schedule/placement/rule_manager.go) | 68,69,95 | `d601f3dba8f99d21b24141a01836d5898edc1e3d72ef093b4a4f1a96d4dbc235` |
| [`pkg/schedule/placement/rule_manager.rs`](../../../pkg/schedule/placement/rule_manager.rs) | 68,69,95 | `d7040660b6db7a23f9d23dea59b3986db8246c4d4c92b25b11bb8fd10d015f3f` |
| [`pkg/schedule/placement/rule_manager.rs.md`](../../../pkg/schedule/placement/rule_manager.rs.md) | 68,69,95 | `a41c35ac8a6efff302359ba20a94c6f18ce6d65ab6cd13f89bb7a93f53492064` |
| [`pkg/schedule/placement/rule_manager_test.go`](../../../pkg/schedule/placement/rule_manager_test.go) | 68,69,95 | `1c26a34d99975f67c55fa8d3a33f5342b9f917634662c3930bd050b3889c89b3` |
| [`pkg/schedule/placement/rule_query.rs`](../../../pkg/schedule/placement/rule_query.rs) | 68,69,95 | `4e1e6bcfa3a15236be4fab248d5a23063a0978eb4a98a5f4fef6db0f1156bcf2` |
| [`pkg/schedule/placement/rule_query.rs.md`](../../../pkg/schedule/placement/rule_query.rs.md) | 68,69,95 | `7a6a72ac187821e5adfdd5b1281b24f2cb031de49f2b62874291bdee2d149ef0` |
| [`pkg/schedule/placement/rule_test.go`](../../../pkg/schedule/placement/rule_test.go) | 68,69,95 | `b408720f205fb1a213fd2940571ff76eade95812d0327a56c57982c2a7ab1fc4` |
| [`pkg/schedule/plan/Cargo.toml`](../../../pkg/schedule/plan/Cargo.toml) | 95 | `a59b18a3cb098fe6d807d5db407afb21c4447ea7dc3cd90bace395e3deba25b3` |
| [`pkg/schedule/plan/balance_plan.go`](../../../pkg/schedule/plan/balance_plan.go) | 95 | `4f861f5afdf2fd71af0b1ea0aef20eab3d7dc40e2165a330487a2ffcca2e118f` |
| [`pkg/schedule/plan/balance_plan.rs`](../../../pkg/schedule/plan/balance_plan.rs) | 95 | `892ffdc77612f2efd27a762e574c91472b26e2200c7d2227ee657be8323af67e` |
| [`pkg/schedule/plan/balance_plan.rs.md`](../../../pkg/schedule/plan/balance_plan.rs.md) | 95 | `0c63c8f0cc3b2cc3307f24db78c2712f3ad906602a355d431a647adcf8540677` |
| [`pkg/schedule/plan/balance_plan_test.go`](../../../pkg/schedule/plan/balance_plan_test.go) | 95 | `025ea4ba854017c871b38cc25a727e3c5eb24da9781666fdf1d2cb8ba6727ff0` |
| [`pkg/schedule/plan/balance_summary.rs`](../../../pkg/schedule/plan/balance_summary.rs) | 95 | `7f51fdfd6891f908eb19c3491c617b946d14ede89681e50579541df794f5bff1` |
| [`pkg/schedule/plan/balance_summary.rs.md`](../../../pkg/schedule/plan/balance_summary.rs.md) | 95 | `d50e4dfd32152d17de637245c60ff99b65684938c9cf2b6e2161968bff3d3916` |
| [`pkg/schedule/plan/collector.rs`](../../../pkg/schedule/plan/collector.rs) | 95 | `3901e5c3811d72b70b9f506a950811ed01728cfc48d24e53ae2c887eaba87018` |
| [`pkg/schedule/plan/collector.rs.md`](../../../pkg/schedule/plan/collector.rs.md) | 95 | `155d8ac683169f6c7df05e19edb3dca58178c1c57ec72bc45a4e181a026c3477` |
| [`pkg/schedule/plan/fixtures/status.tsv`](../../../pkg/schedule/plan/fixtures/status.tsv) | 95 | `67d1223d1857dee4c5dfaf44826ee680400257625d6f66508c752a570b3aa38a` |
| [`pkg/schedule/plan/lib.rs`](../../../pkg/schedule/plan/lib.rs) | 95 | `8d593fdad82375c426d048951f98d7e0b9d5a832da381e3480e1a1c3a3f37a8f` |
| [`pkg/schedule/plan/plan.go`](../../../pkg/schedule/plan/plan.go) | 95 | `2b1edee3759a54db2784854b28b743d4ae996b5ba2e197f9818ac88150c0f34c` |
| [`pkg/schedule/plan/plan_collect_test.rs`](../../../pkg/schedule/plan/plan_collect_test.rs) | 95 | `1b94044d5dad39bed2db465e019d66737a39451a41dd60189d91e54335590ce0` |
| [`pkg/schedule/plan/plan_status_test.rs`](../../../pkg/schedule/plan/plan_status_test.rs) | 95 | `6fe44e4a0b0a29d7afc57885a52e215ac016d9e308bde0c3fb0b77c204717dbe` |
| [`pkg/schedule/plan/plan_summary_test.rs`](../../../pkg/schedule/plan/plan_summary_test.rs) | 95 | `9a6ddbc40e212635e620987317a20272b1838a0b1b91107ba8b7fec7d09c07e1` |
| [`pkg/schedule/plan/status.go`](../../../pkg/schedule/plan/status.go) | 95 | `91175135cf008ae3985e5dab770a1c3f0cc59ff65aac48fd3514aa3ff0ac5537` |
| [`pkg/schedule/plan/status.rs`](../../../pkg/schedule/plan/status.rs) | 95 | `3a6792328778e8712e501ea1a60a4fb0e09d2892f1e919dd443de6234228c24f` |
| [`pkg/schedule/plan/status.rs.md`](../../../pkg/schedule/plan/status.rs.md) | 95 | `d8b9db17db194c1a10faedc87130113302af458cfa59b560e5ae02584902d9c5` |
| [`pkg/schedule/plugin_boundary.rs`](../../../pkg/schedule/plugin_boundary.rs) | 95 | `92e2f4df46e122cb146f82ca10d66b186381e8ef4fd4a4522fa637d884977b80` |
| [`pkg/schedule/plugin_boundary.rs.md`](../../../pkg/schedule/plugin_boundary.rs.md) | 95 | `b203b226db56a3f5839c76584c9d09c15bbf3d6baac316091c44d3b988489d34` |
| [`pkg/schedule/plugin_interface.go`](../../../pkg/schedule/plugin_interface.go) | 95 | `a19830de2352f606ee569137f9e2fd0ad02b6f1db8df816660aef12b795a342f` |
| [`pkg/schedule/prepare.rs`](../../../pkg/schedule/prepare.rs) | 95 | `3e757d42d63c0fafa2f26230e811fdb02076d014f615275c9280407fead2c1d3` |
| [`pkg/schedule/prepare.rs.md`](../../../pkg/schedule/prepare.rs.md) | 95 | `1ddc959ea9fd510143187a34b6dbc4aed2e22e05f558d1e30c7f3c14f240da66` |
| [`pkg/schedule/prepare_checker.go`](../../../pkg/schedule/prepare_checker.go) | 95 | `7ce7563cdec4e4369e42be576a0a6eb955f9f83e40ed5f6616899c3f5cade0d2` |
| [`pkg/schedule/rangelist/Cargo.toml`](../../../pkg/schedule/rangelist/Cargo.toml) | 95 | `b1c6500e07278ece7f614e938c130524f83c9b0bc3701de90b9b6413ebec1e6d` |
| [`pkg/schedule/rangelist/GO-SORT-LICENSE`](../../../pkg/schedule/rangelist/GO-SORT-LICENSE) | 95 | `911f8f5782931320f5b8d1160a76365b83aea6447ee6c04fa6d5591467db9dad` |
| [`pkg/schedule/rangelist/builder.go`](../../../pkg/schedule/rangelist/builder.go) | 95 | `8e47f82617c23ab161b63e5103ef5f52042315fed8390cbee097ff19973df108` |
| [`pkg/schedule/rangelist/builder.rs`](../../../pkg/schedule/rangelist/builder.rs) | 95 | `46a5e3fece5e2d7bcb674d26c97143d9c7c939b72051f7bd31468324b8fec941` |
| [`pkg/schedule/rangelist/builder.rs.md`](../../../pkg/schedule/rangelist/builder.rs.md) | 95 | `f8c4f3df902e592f14e7478cdf87beff8e1c3dd3de1180ad80a1e9a9aa73ffa0` |
| [`pkg/schedule/rangelist/event_sort.rs`](../../../pkg/schedule/rangelist/event_sort.rs) | 95 | `c277dc1eefa12ca4506ba370ebfd144941a4367f76d83d5982ae58ab41009339` |
| [`pkg/schedule/rangelist/event_sort.rs.md`](../../../pkg/schedule/rangelist/event_sort.rs.md) | 95 | `9795652def1862bf5476f83511014e68564b0d170c5b5d053951acac0f3c3b25` |
| [`pkg/schedule/rangelist/lib.rs`](../../../pkg/schedule/rangelist/lib.rs) | 95 | `700681e99accbeb7d8cb4cf54dd88b051241be7cd5738700ada758674a69f212` |
| [`pkg/schedule/rangelist/lib.rs.md`](../../../pkg/schedule/rangelist/lib.rs.md) | 95 | `d15ba121322a3e6d4555b78ba4cf4e0b5df78f24f5cb630038536c74e3e6d0f8` |
| [`pkg/schedule/rangelist/range_build_test.rs`](../../../pkg/schedule/rangelist/range_build_test.rs) | 95 | `2c80f93c8da2df908080a772b313ed9d746547847e577dd7c8976729e9d9dcea` |
| [`pkg/schedule/rangelist/range_list.go`](../../../pkg/schedule/rangelist/range_list.go) | 95 | `45f760a442af79612819cda2e9dc155870beb5c1186eb4215c46e25d7e2e3314` |
| [`pkg/schedule/rangelist/range_list.rs`](../../../pkg/schedule/rangelist/range_list.rs) | 95 | `3dbc0f3d447581ea53c16b8c31569aa4c3abec585cc6be7962343d31981fb9b2` |
| [`pkg/schedule/rangelist/range_list.rs.md`](../../../pkg/schedule/rangelist/range_list.rs.md) | 95 | `7c422bac7a27efd23e43c9a052199794015ac1baa7f619e162b9c399c54cff0b` |
| [`pkg/schedule/rangelist/range_list_test.go`](../../../pkg/schedule/rangelist/range_list_test.go) | 95 | `781c645e201751aa735b7df92bfdb056fad658f37e8109b59b75d247074cca98` |
| [`pkg/schedule/rangelist/range_queries_test.rs`](../../../pkg/schedule/rangelist/range_queries_test.rs) | 95 | `23733eab3d75cded3c1ff68117f386ee36dd19485618c1854fe75ca0ba053579` |
| [`pkg/schedule/scatter/Cargo.toml`](../../../pkg/schedule/scatter/Cargo.toml) | 29,95 | `e1309d57bece726ce5329f6a2354dd1fb94cf2ea98120ada302aaac4cc4c6910` |
| [`pkg/schedule/scatter/lib.rs`](../../../pkg/schedule/scatter/lib.rs) | 29,95 | `d3e92417c697bc345e63508ad3e124b66c6980cb2f0ecaa98dde8e09ef3ae289` |
| [`pkg/schedule/scatter/metrics.go`](../../../pkg/schedule/scatter/metrics.go) | 29,95 | `f43418291a7bd618e02b5e01d206772d6252225e6dbb0d07cff4ab651762a37d` |
| [`pkg/schedule/scatter/region_scatterer.go`](../../../pkg/schedule/scatter/region_scatterer.go) | 29,95 | `9f2f77a2ca302d6896d8dbffc7a736d411133d8df078807fcb8188b1fc75d3f6` |
| [`pkg/schedule/scatter/region_scatterer_test.go`](../../../pkg/schedule/scatter/region_scatterer_test.go) | 29,95 | `c1e78eab5795f607cd1fe83b6130d57f77257daf694b93bcedc25e9aa4dfe67e` |
| [`pkg/schedule/scatter/scatter_batch.rs`](../../../pkg/schedule/scatter/scatter_batch.rs) | 29,95 | `62dae353b8b93b2a75e6db96c48b0dfb934f703a1fc224b4027a2922ccb792d2` |
| [`pkg/schedule/scatter/scatter_batch.rs.md`](../../../pkg/schedule/scatter/scatter_batch.rs.md) | 29,95 | `1d178208954ea96f82b18d4682ba6f4a602f69c1666e5a02eb9805e9b596ecf6` |
| [`pkg/schedule/scatter/scatter_operator.rs`](../../../pkg/schedule/scatter/scatter_operator.rs) | 29,95 | `acf6415cf3eaf4166b33dcdd2814f33c6f6f93280c3247a30a4e02061d5df3dc` |
| [`pkg/schedule/scatter/scatter_operator.rs.md`](../../../pkg/schedule/scatter/scatter_operator.rs.md) | 29,95 | `c18b5fe49049d0ed6c0fabe793491380b2c01aa70db064273e24f0f1dbc9782e` |
| [`pkg/schedule/scatter/scatter_target.rs`](../../../pkg/schedule/scatter/scatter_target.rs) | 29,95 | `0c7c7297cc837a97533172d392d38ecdf6847e725a60fc91ec2fd41c935a3767` |
| [`pkg/schedule/scatter/scatter_target.rs.md`](../../../pkg/schedule/scatter/scatter_target.rs.md) | 29,95 | `886e692d5919fb6c4a3f852e65315510f8dd4d4e1a2042413b849fa401253e84` |
| [`pkg/schedule/scatter/task_059_test.rs`](../../../pkg/schedule/scatter/task_059_test.rs) | 29,95 | `e77781a7b57d91d01863520a30de7c1597ce10e7fdb81771b0d9e7ac8fb9c9b0` |
| [`pkg/schedule/scatter/task_060_test.rs`](../../../pkg/schedule/scatter/task_060_test.rs) | 29,95 | `44cf178b362230ddba46d110dd55e2a75c971da28f701636b961b28a58e612b3` |
| [`pkg/schedule/scatter/task_061_test.rs`](../../../pkg/schedule/scatter/task_061_test.rs) | 29,95 | `1ed20e3e179b566eb97f55552696afc0cff658e3d689ae2bbf3312515d633761` |
| [`pkg/schedule/scheduler_restore.rs`](../../../pkg/schedule/scheduler_restore.rs) | 17,95 | `6045035160eff19cbae286a6a21371b6b26930b6fbd306e7b499580142c3ed1a` |
| [`pkg/schedule/scheduler_restore.rs.md`](../../../pkg/schedule/scheduler_restore.rs.md) | 95 | `34d40af5dd0b1b37e57d62995c49071258aee419e326c4ae065afeaaf763c48c` |
| [`pkg/schedule/schedulers/Cargo.toml`](../../../pkg/schedule/schedulers/Cargo.toml) | 95 | `4e44f9f8013a885ee182285489c50e15935258f9079e1e2c5d75790b9315230a` |
| [`pkg/schedule/schedulers/OWNERS`](../../../pkg/schedule/schedulers/OWNERS) | 95 | `53a15eae7547d0984e5258d6084d634287de91e2a44da44d9bbc0319978ebd60` |
| [`pkg/schedule/schedulers/balance_benchmark_test.go`](../../../pkg/schedule/schedulers/balance_benchmark_test.go) | 95 | `c7c2ba0bfa97690dc3e8645e5470e853eb21da2dba515795de8d0a945cbdb3fa` |
| [`pkg/schedule/schedulers/balance_leader.go`](../../../pkg/schedule/schedulers/balance_leader.go) | 95 | `904edff2c88b23fc228cd7d54b5054067c82254adc64855e47d1503e074c7a9f` |
| [`pkg/schedule/schedulers/balance_leader.rs`](../../../pkg/schedule/schedulers/balance_leader.rs) | 95 | `f7fa7e443c6323507f4b5ee6853cf85d45a1dcb921c49999c8338d525dba75a1` |
| [`pkg/schedule/schedulers/balance_leader.rs.md`](../../../pkg/schedule/schedulers/balance_leader.rs.md) | 95 | `1fe6489828594a1263747b8a7147c71aa266455dc7741f4e88ea024cf0f3655e` |
| [`pkg/schedule/schedulers/balance_leader_config.rs`](../../../pkg/schedule/schedulers/balance_leader_config.rs) | 95 | `5cbc88aa45e4d19970269fd9bfc8b0f3602b313ad5d0def44a60560a378d55f0` |
| [`pkg/schedule/schedulers/balance_leader_config.rs.md`](../../../pkg/schedule/schedulers/balance_leader_config.rs.md) | 95 | `e7d06af59a8a80d15ca6142a9c8aca75fd88a2bd995d1c5440e8e780014296b5` |
| [`pkg/schedule/schedulers/balance_leader_test.go`](../../../pkg/schedule/schedulers/balance_leader_test.go) | 95 | `c9b79231c539c2d0e8322f372ea1df30c950e41213cdd313e7e4f679a2858bb5` |
| [`pkg/schedule/schedulers/balance_range.go`](../../../pkg/schedule/schedulers/balance_range.go) | 95 | `ccd59a50facd035b9f50f69a3ac1e4a538540cbfa0fc0916840f0392224cdc2d` |
| [`pkg/schedule/schedulers/balance_range_job.rs`](../../../pkg/schedule/schedulers/balance_range_job.rs) | 95 | `4902bde9dc9a1a5b0bfcf133fe353aed08aa366f8731c574abdcd29ae0d2712b` |
| [`pkg/schedule/schedulers/balance_range_job.rs.md`](../../../pkg/schedule/schedulers/balance_range_job.rs.md) | 95 | `f23b96bafa93dcb4f50bacc410148a54dba0870e1aca67ef553d341e1eba2d41` |
| [`pkg/schedule/schedulers/balance_range_schedule.rs`](../../../pkg/schedule/schedulers/balance_range_schedule.rs) | 95 | `3b887d9979ade236558f0ba34534cae665bd355d685223bb3b0ce97a782985d2` |
| [`pkg/schedule/schedulers/balance_range_schedule.rs.md`](../../../pkg/schedule/schedulers/balance_range_schedule.rs.md) | 95 | `10d552e935bd5263466119bb395394b2627f332047cb73aa61b75cf48e3e8a13` |
| [`pkg/schedule/schedulers/balance_range_test.go`](../../../pkg/schedule/schedulers/balance_range_test.go) | 95 | `a684a154f0b8faff1717551d3b92c2db17d8cf9de6cdb3e1f286009e00c8bcc1` |
| [`pkg/schedule/schedulers/balance_region.go`](../../../pkg/schedule/schedulers/balance_region.go) | 95 | `616ed0bb53b58909c013e10be1000822fe0ca72ff46d36ec0aaab1031213c1fc` |
| [`pkg/schedule/schedulers/balance_region.rs`](../../../pkg/schedule/schedulers/balance_region.rs) | 95 | `21c088a65b7f4b3e29b9abdd9aad5db6e3452a5ad203b4ae0480bf19ee7235de` |
| [`pkg/schedule/schedulers/balance_region.rs.md`](../../../pkg/schedule/schedulers/balance_region.rs.md) | 95 | `1fa3ee4798b63be9a83da7a146cae5a86156b17f1e34b194e143c0347d3514d7` |
| [`pkg/schedule/schedulers/balance_region_test.go`](../../../pkg/schedule/schedulers/balance_region_test.go) | 95 | `a0037629e3317d68ae59e4ed07ed9bb3859045aa57901151a7075707a17c4532` |
| [`pkg/schedule/schedulers/balance_solver.rs`](../../../pkg/schedule/schedulers/balance_solver.rs) | 95 | `3fdf6adcf42cf0f5fcbf53960ccad8c826a1ffbff50eca97a48f643f26c94d84` |
| [`pkg/schedule/schedulers/balance_solver.rs.md`](../../../pkg/schedule/schedulers/balance_solver.rs.md) | 95 | `5f29df3c13bcdab54b15b8ab2eda0198fc42277d9511c8a19233633bd07d1b0c` |
| [`pkg/schedule/schedulers/balance_witness.go`](../../../pkg/schedule/schedulers/balance_witness.go) | 95 | `c0b444018eeb1847513516cd79548fdceb0cd0e1fdd9d9269f33aca7f9c5a352` |
| [`pkg/schedule/schedulers/balance_witness.rs`](../../../pkg/schedule/schedulers/balance_witness.rs) | 95 | `8a785651d80e2d9cbf7a35bd3ae3a75dfcbc9d9087b11cdbba84bd8fad167b04` |
| [`pkg/schedule/schedulers/balance_witness.rs.md`](../../../pkg/schedule/schedulers/balance_witness.rs.md) | 95 | `3dacc374734f4a1074efc82f2769d9f5d2fdeb7dea20f336c417ea67938e39d6` |
| [`pkg/schedule/schedulers/balance_witness_config.rs`](../../../pkg/schedule/schedulers/balance_witness_config.rs) | 95 | `c7454cb48c07868361263a39748cbe4ecc548f509a960de6de54f95eb4bdbd56` |
| [`pkg/schedule/schedulers/balance_witness_config.rs.md`](../../../pkg/schedule/schedulers/balance_witness_config.rs.md) | 95 | `6b5673a6bf677b0f2fbf2540864f0dd8cb4490546485b8a10823625a66b1f282` |
| [`pkg/schedule/schedulers/balance_witness_test.go`](../../../pkg/schedule/schedulers/balance_witness_test.go) | 95 | `cecd97b88b667ebe6a135d72688962f9897c696c0cf9b5ab67ede7aa26e36d80` |
| [`pkg/schedule/schedulers/base.rs`](../../../pkg/schedule/schedulers/base.rs) | 95 | `2e630db59e3f1f9260d6a984edf09dbf542966f14ec746a7826e113c23b0d7a5` |
| [`pkg/schedule/schedulers/base.rs.md`](../../../pkg/schedule/schedulers/base.rs.md) | 95 | `ab795123eab9ecd311f559412350627f899a32da68e0018df8df4d46dda7ba83` |
| [`pkg/schedule/schedulers/base_scheduler.go`](../../../pkg/schedule/schedulers/base_scheduler.go) | 95 | `1da013a2323c2756dcfd3b1203e91c481c58e80e258e3848d5db23dd261e6bdc` |
| [`pkg/schedule/schedulers/config.go`](../../../pkg/schedule/schedulers/config.go) | 95 | `64024b895d0b48c030acd9684c3879993710856796eebe11595d4108073b6409` |
| [`pkg/schedule/schedulers/config.rs`](../../../pkg/schedule/schedulers/config.rs) | 95 | `df09ddc16ac13cba9471c994f9317d1a7987206b2ce9d4cebd1c26b24baca95b` |
| [`pkg/schedule/schedulers/config.rs.md`](../../../pkg/schedule/schedulers/config.rs.md) | 95 | `22fe2d2e58b75c47db902a7bf89b3d0002f8b34bece9181d769a7e5292896d3a` |
| [`pkg/schedule/schedulers/config_test.go`](../../../pkg/schedule/schedulers/config_test.go) | 95 | `a258f917ed81f3d95934a5c87130ab7d84f604ee6c4555e9dc99cbe3058c1055` |
| [`pkg/schedule/schedulers/diagnostic_record.rs`](../../../pkg/schedule/schedulers/diagnostic_record.rs) | 95 | `08842ef8ba575a92633cddbcc40acd8c0ed5adf0aaf86fb547e3eed79fd966f6` |
| [`pkg/schedule/schedulers/diagnostic_record.rs.md`](../../../pkg/schedule/schedulers/diagnostic_record.rs.md) | 95 | `b05e2965b41efebc0ce410ef3fa2f590878f1107fdcafe2ab3d05e723193b530` |
| [`pkg/schedule/schedulers/diagnostic_recorder.go`](../../../pkg/schedule/schedulers/diagnostic_recorder.go) | 95 | `4221cf1359bb837642986ad82b3db18bfc1423ce5eddfd2a378c18f3f08730e4` |
| [`pkg/schedule/schedulers/evict_disk.rs`](../../../pkg/schedule/schedulers/evict_disk.rs) | 95 | `ac5fabb17c2155a37cfc86a3852235086cee5ee30f7d76de086411ed75f69561` |
| [`pkg/schedule/schedulers/evict_disk.rs.md`](../../../pkg/schedule/schedulers/evict_disk.rs.md) | 95 | `87bbe943cfaf57eda1979da91d2c450a5b31c53eef2fc35257c4f69c16dea4eb` |
| [`pkg/schedule/schedulers/evict_leader.go`](../../../pkg/schedule/schedulers/evict_leader.go) | 95 | `7fb5a50e54d33f0069e22c69ffd1341ae64f6310a5d1c49ac315aad6f925a460` |
| [`pkg/schedule/schedulers/evict_leader.rs`](../../../pkg/schedule/schedulers/evict_leader.rs) | 95 | `a14dc069105a919054cc55ccab7fcffe793d32b43c7e0b331cbd951aa27de917` |
| [`pkg/schedule/schedulers/evict_leader.rs.md`](../../../pkg/schedule/schedulers/evict_leader.rs.md) | 95 | `96938e068642f106793e3ef737c53a2e76c8bbdadf6a92b617bc64e129c778c7` |
| [`pkg/schedule/schedulers/evict_leader_config.rs`](../../../pkg/schedule/schedulers/evict_leader_config.rs) | 95 | `f1034cacd77f202debaca1812fb4b370863d5a574c15db585b551e3b89ede688` |
| [`pkg/schedule/schedulers/evict_leader_config.rs.md`](../../../pkg/schedule/schedulers/evict_leader_config.rs.md) | 95 | `335675ec51a6a91119dc461448ff451101746151ab76e1bb05d352f5308ee494` |
| [`pkg/schedule/schedulers/evict_leader_test.go`](../../../pkg/schedule/schedulers/evict_leader_test.go) | 95 | `eb9288a2cb678c8e7911b876788b537fdce89e13ff0aa1d7543c760e74454b21` |
| [`pkg/schedule/schedulers/evict_network.rs`](../../../pkg/schedule/schedulers/evict_network.rs) | 95 | `15b0aa19b3004f73e5883de09cd06143a3c171c332aa4c91d837f874069ef648` |
| [`pkg/schedule/schedulers/evict_network.rs.md`](../../../pkg/schedule/schedulers/evict_network.rs.md) | 95 | `3e628db6966a98249c793f4f04aabe6b7215ccb81d28d9d790170452ade420f6` |
| [`pkg/schedule/schedulers/evict_network_detect.rs`](../../../pkg/schedule/schedulers/evict_network_detect.rs) | 95 | `ea9a3f75dfc8dce604149f9ac758636358badddcaeb4a9f4737ae24370ad0025` |
| [`pkg/schedule/schedulers/evict_network_detect.rs.md`](../../../pkg/schedule/schedulers/evict_network_detect.rs.md) | 95 | `b014d883b19582531da1ecfac705e0f673a943079a5c888143680e9fec809eb3` |
| [`pkg/schedule/schedulers/evict_slow_store.go`](../../../pkg/schedule/schedulers/evict_slow_store.go) | 95 | `dc5a6f0d9f59e6001c0882d54d16771515266b23cd43d1c3f9a43520e217aeda` |
| [`pkg/schedule/schedulers/evict_slow_store_test.go`](../../../pkg/schedule/schedulers/evict_slow_store_test.go) | 95 | `1c23c9d988c0c84b452fee31cc6d15d95badcb1ae7ad6b7e32323a222b7624e5` |
| [`pkg/schedule/schedulers/evict_slow_trend.go`](../../../pkg/schedule/schedulers/evict_slow_trend.go) | 95 | `cc8c13a96b1dd341a7c3a24d15c2a8ad7ed129434ee9c0f2e7f8a60edefa6fa6` |
| [`pkg/schedule/schedulers/evict_slow_trend_test.go`](../../../pkg/schedule/schedulers/evict_slow_trend_test.go) | 95 | `e3efaf585ca813dc16a183a0d32be59e5b53c2d53967956bee65fe847cc9bb08` |
| [`pkg/schedule/schedulers/evict_stopping.rs`](../../../pkg/schedule/schedulers/evict_stopping.rs) | 95 | `89b4d21a856b2d40d04b11a22372894477cc9b38824551c37b08912e73d04088` |
| [`pkg/schedule/schedulers/evict_stopping.rs.md`](../../../pkg/schedule/schedulers/evict_stopping.rs.md) | 95 | `9586e7d2b5fd9805e06981fd571ac78ed6232134436d9258ae02a29c87f4fac1` |
| [`pkg/schedule/schedulers/evict_stopping_store.go`](../../../pkg/schedule/schedulers/evict_stopping_store.go) | 95 | `297122adad26bc7ad43d5543e6b3f8566a4de8ddf26191e65863ef89ab012bd3` |
| [`pkg/schedule/schedulers/evict_stopping_store_test.go`](../../../pkg/schedule/schedulers/evict_stopping_store_test.go) | 95 | `19dddc81278efe22f9c5c7e091571be64ffe51597103d091f015a76ae35e948c` |
| [`pkg/schedule/schedulers/grant_hot.rs`](../../../pkg/schedule/schedulers/grant_hot.rs) | 95 | `6bf962cd163cf94d50d94fe1f2a21aee930f639e4d2fe7c2a3e49947d2a52322` |
| [`pkg/schedule/schedulers/grant_hot.rs.md`](../../../pkg/schedule/schedulers/grant_hot.rs.md) | 95 | `ebb17b8d3cd6c83963ef246fc46c1990ed9c1ebe912c916dec387e2e394cd47d` |
| [`pkg/schedule/schedulers/grant_hot_region.go`](../../../pkg/schedule/schedulers/grant_hot_region.go) | 95 | `2e19e15d5305f8a09c62639be401dce2f11f6e83eab433bac4f56f9f9690dcd5` |
| [`pkg/schedule/schedulers/grant_hot_region_test.go`](../../../pkg/schedule/schedulers/grant_hot_region_test.go) | 95 | `e171cc11d1fd9f3a874eda88a4b744a19fe38e59e7129c8274745ae497293914` |
| [`pkg/schedule/schedulers/grant_leader.go`](../../../pkg/schedule/schedulers/grant_leader.go) | 95 | `4c1f9fda46d697bce4c22ce8dbb07b8a7f255d5d475028067e7c06c8c3aca95f` |
| [`pkg/schedule/schedulers/grant_leader.rs`](../../../pkg/schedule/schedulers/grant_leader.rs) | 95 | `4e272095fba349a193833a715438dcecf965e86e82988ea2f0796e37f7bd6291` |
| [`pkg/schedule/schedulers/grant_leader.rs.md`](../../../pkg/schedule/schedulers/grant_leader.rs.md) | 95 | `322b137202030d3ba38fd1df8cbb44c5ca97ef26087fb063c818818222530b5b` |
| [`pkg/schedule/schedulers/grant_leader_config.rs`](../../../pkg/schedule/schedulers/grant_leader_config.rs) | 95 | `0e1d96764eb7700d33cef06b06091e97293a51e2c98bf5331fe86cab911ade68` |
| [`pkg/schedule/schedulers/grant_leader_config.rs.md`](../../../pkg/schedule/schedulers/grant_leader_config.rs.md) | 95 | `1127c887e4fe3cb372a5888cbb641af7fd6faffad44b87ae4f9be3eed26eaf23` |
| [`pkg/schedule/schedulers/grant_leader_test.go`](../../../pkg/schedule/schedulers/grant_leader_test.go) | 95 | `f1763d8d73e99c470933119be6b42bcfc892a17b7e7782109bc7622c22b4cb32` |
| [`pkg/schedule/schedulers/hot_config.rs`](../../../pkg/schedule/schedulers/hot_config.rs) | 95 | `6087cf0b583d6523b0aa16e32bb439e49a1d52181da5595cbe4fd0969ba06366` |
| [`pkg/schedule/schedulers/hot_config.rs.md`](../../../pkg/schedule/schedulers/hot_config.rs.md) | 95 | `a5ca93a9c36e3ec967713e582b0e5603105e38d2e2e4a2a1e07a1c280befea74` |
| [`pkg/schedule/schedulers/hot_destination.rs`](../../../pkg/schedule/schedulers/hot_destination.rs) | 95 | `28bb09264df5bff755d4d476dfdc1571d20a069e41a420ed66dcba865fe639f3` |
| [`pkg/schedule/schedulers/hot_destination.rs.md`](../../../pkg/schedule/schedulers/hot_destination.rs.md) | 95 | `a4b4c32fa75d8e0240c074bc4f8d10ba2a1411ed546f4b839f64ec14204e0507` |
| [`pkg/schedule/schedulers/hot_pending.rs`](../../../pkg/schedule/schedulers/hot_pending.rs) | 95 | `31f02c7e4b21026281acb6e02aa3424e13fbadc02c9970bdbb7e84fd5f152af4` |
| [`pkg/schedule/schedulers/hot_pending.rs.md`](../../../pkg/schedule/schedulers/hot_pending.rs.md) | 95 | `88c6c569269d517d3db97745e96c63b10fb2965db0afb150a1fa83e5250389a6` |
| [`pkg/schedule/schedulers/hot_placement.rs`](../../../pkg/schedule/schedulers/hot_placement.rs) | 95 | `2e0ff2e934d42cb4d7ea24510b2f7135efde1129d6b2b9a437462aad99872009` |
| [`pkg/schedule/schedulers/hot_placement.rs.md`](../../../pkg/schedule/schedulers/hot_placement.rs.md) | 95 | `81205ab8a8a5ce42fafbff5e91dd668f71111830881eb4d59f27d2e8737807e8` |
| [`pkg/schedule/schedulers/hot_rank_v1.rs`](../../../pkg/schedule/schedulers/hot_rank_v1.rs) | 95 | `db205fdce7e6fe31201c6ed5a7e5b60a5b79e1d795dd6f77b34335a286946dce` |
| [`pkg/schedule/schedulers/hot_rank_v1.rs.md`](../../../pkg/schedule/schedulers/hot_rank_v1.rs.md) | 95 | `4b81ddd0d4bc0e513e9e92254bd4775f5de6e6ec8f24cb21a6cf1346cbc473c5` |
| [`pkg/schedule/schedulers/hot_rank_v2.rs`](../../../pkg/schedule/schedulers/hot_rank_v2.rs) | 95 | `6e72f193207b0ef67f1f013e0a039e76dd1b819604f1ca23c3ceeaca2af0705a` |
| [`pkg/schedule/schedulers/hot_rank_v2.rs.md`](../../../pkg/schedule/schedulers/hot_rank_v2.rs.md) | 95 | `01ae2adcd23ddb76f554edeec9f493cf9f66f4409db8d5b914c9a64cab14b7d5` |
| [`pkg/schedule/schedulers/hot_region.go`](../../../pkg/schedule/schedulers/hot_region.go) | 95 | `2e34a6068d48c1ffed3a78764fc18843b20b6fab2943587df4e907acfe692ad6` |
| [`pkg/schedule/schedulers/hot_region_config.go`](../../../pkg/schedule/schedulers/hot_region_config.go) | 95 | `189283bad7801de492dcf5e2866a37118e1802f37250583d775c22a484fbaa3b` |
| [`pkg/schedule/schedulers/hot_region_config_test.go`](../../../pkg/schedule/schedulers/hot_region_config_test.go) | 95 | `33fefec24efd470ff8d1bd70f15bf889b47148587636507cbc76ef4f57f5b87e` |
| [`pkg/schedule/schedulers/hot_region_rank_v1.go`](../../../pkg/schedule/schedulers/hot_region_rank_v1.go) | 95 | `d37b60b6506fd03d308e398a039dfe6884972103f1c58b268f11356e84d862df` |
| [`pkg/schedule/schedulers/hot_region_rank_v2.go`](../../../pkg/schedule/schedulers/hot_region_rank_v2.go) | 95 | `63fe4d9673afbd47d4905abb2c97e1d9590f9b34b61d3b37537469896f9a9e61` |
| [`pkg/schedule/schedulers/hot_region_rank_v2_test.go`](../../../pkg/schedule/schedulers/hot_region_rank_v2_test.go) | 95 | `d02a4219dc8c4cbbf9d9e0061013242155544ad693d3ebde00b36c6417ec3d2d` |
| [`pkg/schedule/schedulers/hot_region_solution.go`](../../../pkg/schedule/schedulers/hot_region_solution.go) | 95 | `cd6c21a14bf0249dc177c764de7be8125294409670bfe2ded31f29364e42505d` |
| [`pkg/schedule/schedulers/hot_region_solver.go`](../../../pkg/schedule/schedulers/hot_region_solver.go) | 95 | `f71cb92668e2221aa0e6e9ff09004cafcea4ded55ad344d80e5d8f46a88df6ce` |
| [`pkg/schedule/schedulers/hot_region_solver_test.go`](../../../pkg/schedule/schedulers/hot_region_solver_test.go) | 95 | `80697f96cf32436ef1d26b2d592e6ccba1b3890fdc90eecb146fac7aee9f31c3` |
| [`pkg/schedule/schedulers/hot_region_test.go`](../../../pkg/schedule/schedulers/hot_region_test.go) | 95 | `20e49d3e00501845befb864375ed74b8e2da5f82dc8e71006c496463fb08a756` |
| [`pkg/schedule/schedulers/hot_schedule.rs`](../../../pkg/schedule/schedulers/hot_schedule.rs) | 95 | `b84b9b92ca73d8360a14ab55d04d7917a49dff51a7bd348bb69fc82bf782a342` |
| [`pkg/schedule/schedulers/hot_schedule.rs.md`](../../../pkg/schedule/schedulers/hot_schedule.rs.md) | 95 | `bb20eca96d3fd476e50b1c788c84aa0108d981ba032f1f131e1d04aa0cfecc48` |
| [`pkg/schedule/schedulers/hot_solution.rs`](../../../pkg/schedule/schedulers/hot_solution.rs) | 95 | `eea246cb187d14ff779997569773d10ef3a494e9694a9bc8693efa665e0c80f1` |
| [`pkg/schedule/schedulers/hot_solution.rs.md`](../../../pkg/schedule/schedulers/hot_solution.rs.md) | 95 | `0e282d2fa41c8f45ae4be285db45e12fe605765621d6639edc6c38059a9ac80b` |
| [`pkg/schedule/schedulers/hot_solve.rs`](../../../pkg/schedule/schedulers/hot_solve.rs) | 95 | `933569d1d22506a636d50030d01e9fddccf143ad0d03562eff09058be9163b36` |
| [`pkg/schedule/schedulers/hot_solve.rs.md`](../../../pkg/schedule/schedulers/hot_solve.rs.md) | 95 | `a6acf57821207011ae810242266e342850764bd53cfbef5d6f93dcbba37c8998` |
| [`pkg/schedule/schedulers/hot_source.rs`](../../../pkg/schedule/schedulers/hot_source.rs) | 95 | `2e7ac333067751c12a83b7cd480303771b6f051b0ab000bafc71a22a3823d6ec` |
| [`pkg/schedule/schedulers/hot_source.rs.md`](../../../pkg/schedule/schedulers/hot_source.rs.md) | 95 | `4f5646d0a06d373b72f2e2e30cfc9d3508e5d93793b36f0844c6f860ab21c11d` |
| [`pkg/schedule/schedulers/hot_split.rs`](../../../pkg/schedule/schedulers/hot_split.rs) | 95 | `d049358b3b8e83c83e6c4569fb4851d85fcbd6c4993b53f21c5451102f066bff` |
| [`pkg/schedule/schedulers/hot_split.rs.md`](../../../pkg/schedule/schedulers/hot_split.rs.md) | 95 | `c2294f029911debfbc554a30b55d18e5d3b367d212b5b5e30efd1faa9b8442bc` |
| [`pkg/schedule/schedulers/init.go`](../../../pkg/schedule/schedulers/init.go) | 95 | `09664f94f3ebe92a4cad1c9a435fb1f8619c72731e8fa55ec0c75c2818175a40` |
| [`pkg/schedule/schedulers/label.go`](../../../pkg/schedule/schedulers/label.go) | 95 | `c6127510efb37ebafca5db1482dadc0a851b25532c99d1b5962e3f60dc0d25b6` |
| [`pkg/schedule/schedulers/label.rs`](../../../pkg/schedule/schedulers/label.rs) | 95 | `321043e2a001855ca67dc9b1574fb70a8b03cdeed902848ec9199566a0003ad7` |
| [`pkg/schedule/schedulers/label.rs.md`](../../../pkg/schedule/schedulers/label.rs.md) | 95 | `1b50408b9bb205f5490a7434071b92785f4537d53dfd4e73c4b14f20ff9f5f52` |
| [`pkg/schedule/schedulers/lib.rs`](../../../pkg/schedule/schedulers/lib.rs) | 95 | `e2534d748959dd0d160cc9ac8faa3f05149ee237422099c2946954f2a1e8d25a` |
| [`pkg/schedule/schedulers/load_compare.rs`](../../../pkg/schedule/schedulers/load_compare.rs) | 95 | `0b89723b7ef468e0bb5357ce24656f9b4de4d8c8134e628df848c5dba4195d2c` |
| [`pkg/schedule/schedulers/load_compare.rs.md`](../../../pkg/schedule/schedulers/load_compare.rs.md) | 95 | `0636201e9c4fe95de82b774b2def1497ac60b2a796621d7289533665bce540c4` |
| [`pkg/schedule/schedulers/metrics.go`](../../../pkg/schedule/schedulers/metrics.go) | 95 | `add2f1dacf1fbd2f8fadd516ae4ab04895aea7783dc951b53deab1581165869e` |
| [`pkg/schedule/schedulers/random_merge.go`](../../../pkg/schedule/schedulers/random_merge.go) | 95 | `1df788b885604f424a4a8544d7d00e69f1525dc6aae53ce212f87a20bae7410c` |
| [`pkg/schedule/schedulers/random_merge.rs`](../../../pkg/schedule/schedulers/random_merge.rs) | 95 | `bdcc41b89cb821c2d97f5114bfbd94e6d45d10f1d29519d1df6a66695d30cde7` |
| [`pkg/schedule/schedulers/random_merge.rs.md`](../../../pkg/schedule/schedulers/random_merge.rs.md) | 95 | `114a2968cd74c9a02ac5763eaad44e2763eda307d73a6acd1fb7f8332e41153c` |
| [`pkg/schedule/schedulers/random_merge_test.go`](../../../pkg/schedule/schedulers/random_merge_test.go) | 95 | `e45ce6b607561b5bf388167fb9a15ebbc6cbfb3e0d351450a5567a855d221b7e` |
| [`pkg/schedule/schedulers/range_cluster.go`](../../../pkg/schedule/schedulers/range_cluster.go) | 95 | `90cd33aae78e9ff8ca757259586c33b8efe643b56bab286f31e6d20d6a0505c3` |
| [`pkg/schedule/schedulers/range_view.rs`](../../../pkg/schedule/schedulers/range_view.rs) | 95 | `7757d2b32e80c3216dd80e02e66b463fb64dae957e308ecca1af6b3def4080b4` |
| [`pkg/schedule/schedulers/range_view.rs.md`](../../../pkg/schedule/schedulers/range_view.rs.md) | 95 | `db268e65c9cfb4225155ab726c68e66114f84b19511b325b94c64dc15b60b19f` |
| [`pkg/schedule/schedulers/registry.rs`](../../../pkg/schedule/schedulers/registry.rs) | 95 | `f0fb7e3653a6246fd1bbdca6afb6534dcc84ededba79f7b3cbb4e30679ed71f9` |
| [`pkg/schedule/schedulers/registry.rs.md`](../../../pkg/schedule/schedulers/registry.rs.md) | 95 | `86f21942220c1205ed300f029868f0975ca04e9c9be1f3c70dc0567f72c56ed4` |
| [`pkg/schedule/schedulers/scatter_range.go`](../../../pkg/schedule/schedulers/scatter_range.go) | 95 | `49d0f15a24cf8a618d4d83b98f37abddb02db9859a45a3f4036af46e9c0ba45d` |
| [`pkg/schedule/schedulers/scatter_range.rs`](../../../pkg/schedule/schedulers/scatter_range.rs) | 95 | `7de5b31a62bf79bbdde08077c436826f7efe5940e81fa11adbb41bddd750f495` |
| [`pkg/schedule/schedulers/scatter_range.rs.md`](../../../pkg/schedule/schedulers/scatter_range.rs.md) | 95 | `e7befa19fd8e8c00fd3e91bd2d14402ca4be79520a6aa8fd9504b8f486f84e6d` |
| [`pkg/schedule/schedulers/scatter_range_test.go`](../../../pkg/schedule/schedulers/scatter_range_test.go) | 95 | `4f1f561d12c5b0e41de73eb96ebd6868baed7da3a93340efd2ee0f137a70287a` |
| [`pkg/schedule/schedulers/scheduler.go`](../../../pkg/schedule/schedulers/scheduler.go) | 95 | `c0330c4dc38ebb4d7b0e6d5a3b5d1fe5da85a2e4ec9b22a2e90ea9682112365c` |
| [`pkg/schedule/schedulers/scheduler_control.rs`](../../../pkg/schedule/schedulers/scheduler_control.rs) | 95 | `e224d05dd2364a316fa5873ce7a7442203edd97fd5f9281436d5438b98042768` |
| [`pkg/schedule/schedulers/scheduler_control.rs.md`](../../../pkg/schedule/schedulers/scheduler_control.rs.md) | 95 | `9e2f2b058713cdaa204406024861e8c7e8b3992625f926ec98ea16c039fe51db` |
| [`pkg/schedule/schedulers/scheduler_controller.go`](../../../pkg/schedule/schedulers/scheduler_controller.go) | 95 | `5b8ad7382dcf829b9405240fb3e1e72ba659aa5ae301fb0257a3a66c318a7153` |
| [`pkg/schedule/schedulers/scheduler_loop.rs`](../../../pkg/schedule/schedulers/scheduler_loop.rs) | 95 | `57979a157c730c3269218c57ac7a7263acb7c20f4d662cc240c73684e5f1175e` |
| [`pkg/schedule/schedulers/scheduler_loop.rs.md`](../../../pkg/schedule/schedulers/scheduler_loop.rs.md) | 95 | `e6d2e23bcba79e0600190172da649c70077423173bcc6c1c9e3a3be11ccee86c` |
| [`pkg/schedule/schedulers/scheduler_metrics.rs`](../../../pkg/schedule/schedulers/scheduler_metrics.rs) | 95 | `006334ace8a1c1f5e6193df472a5fb2ad6a32422901e53843e989f7a2835a28e` |
| [`pkg/schedule/schedulers/scheduler_metrics.rs.md`](../../../pkg/schedule/schedulers/scheduler_metrics.rs.md) | 95 | `787aae212b1e530dbf50b17f77a58f478b24eb067ba188a3351cbf80eb03dc14` |
| [`pkg/schedule/schedulers/scheduler_test.go`](../../../pkg/schedule/schedulers/scheduler_test.go) | 95 | `55b18eda7e1834ed5090cf360f2cbac1048b0b164e4e84dcf06cbfbbdc8162fc` |
| [`pkg/schedule/schedulers/shuffle_hot.rs`](../../../pkg/schedule/schedulers/shuffle_hot.rs) | 95 | `675b9a6a64e585d68816df8a897789455244c27ab05a84347d48bb7b1d9859ac` |
| [`pkg/schedule/schedulers/shuffle_hot.rs.md`](../../../pkg/schedule/schedulers/shuffle_hot.rs.md) | 95 | `e1cce20d0fea8117d3c41ae5d805017cd82793a63d4e5cf06ae9b26ae0067523` |
| [`pkg/schedule/schedulers/shuffle_hot_region.go`](../../../pkg/schedule/schedulers/shuffle_hot_region.go) | 95 | `a53d67b39bbd2aa67673ca7e58dbc1d39abead235734d5e97fe744d364ca1042` |
| [`pkg/schedule/schedulers/shuffle_leader.go`](../../../pkg/schedule/schedulers/shuffle_leader.go) | 95 | `369f4d3aeebb88eb67cf261c53c0f25e9a887eb654d7a1dd2e6626a465351fa4` |
| [`pkg/schedule/schedulers/shuffle_leader.rs`](../../../pkg/schedule/schedulers/shuffle_leader.rs) | 95 | `d566f507beeb9e667bad9fdf2344bde4522d62f27115efc4c6f25c01584f0527` |
| [`pkg/schedule/schedulers/shuffle_leader.rs.md`](../../../pkg/schedule/schedulers/shuffle_leader.rs.md) | 95 | `c2a6488274bc30c4bdc7a41fd9e52c1e3516d71ad3340167cfbff982a9f6964b` |
| [`pkg/schedule/schedulers/shuffle_region.go`](../../../pkg/schedule/schedulers/shuffle_region.go) | 95 | `e5844a8e1f52a635b9bf5819f73d2819fb22c345957b9b05f4c648113e6da0cf` |
| [`pkg/schedule/schedulers/shuffle_region.rs`](../../../pkg/schedule/schedulers/shuffle_region.rs) | 95 | `9fb28c379064bdfe35fc02f479b9967d45eb1ace40a8518e88c2ea582577c611` |
| [`pkg/schedule/schedulers/shuffle_region.rs.md`](../../../pkg/schedule/schedulers/shuffle_region.rs.md) | 95 | `7761ee4a39d5e5166b45a73f568901fc529bbd2ac4b72447002cc41e9ef9924d` |
| [`pkg/schedule/schedulers/shuffle_region_config.go`](../../../pkg/schedule/schedulers/shuffle_region_config.go) | 95 | `4350643dec0a6cec8d9fdf6dcde757219344dd1a1c30260ebaa500643823924c` |
| [`pkg/schedule/schedulers/slow_trend_candidate.rs`](../../../pkg/schedule/schedulers/slow_trend_candidate.rs) | 95 | `c66ceebf5daf1a2b24bba9108805b1f51678c00935a7f4906793b4acf0994ea7` |
| [`pkg/schedule/schedulers/slow_trend_candidate.rs.md`](../../../pkg/schedule/schedulers/slow_trend_candidate.rs.md) | 95 | `c481ff15f5bf03a1d0eb630677a77310dbf05920b89795ef1f867433245b59bd` |
| [`pkg/schedule/schedulers/slow_trend_recover.rs`](../../../pkg/schedule/schedulers/slow_trend_recover.rs) | 95 | `af5e573dc7f30ee3cc75457d77b9119f3aa51abca353eeeb8a967e6bd36524f8` |
| [`pkg/schedule/schedulers/slow_trend_recover.rs.md`](../../../pkg/schedule/schedulers/slow_trend_recover.rs.md) | 95 | `28f50dc517f2d216a2e63679ed5e06e033e7d9231bc36188f405e0898542c9f3` |
| [`pkg/schedule/schedulers/task_065_test.rs`](../../../pkg/schedule/schedulers/task_065_test.rs) | 95 | `90d248143dc09892f3a326b475762beb5afc5c6c3f0e695e8b46785bfeb3047b` |
| [`pkg/schedule/schedulers/task_066_test.rs`](../../../pkg/schedule/schedulers/task_066_test.rs) | 95 | `c6d3f5dce22d96d1c58a95216edc83309488f295a11cb28bc3d3acfbb041d59c` |
| [`pkg/schedule/schedulers/task_067_test.rs`](../../../pkg/schedule/schedulers/task_067_test.rs) | 95 | `92c4ef1ec774a205b06b445f2140f65a8275e8b87237fc0047e2d3a73af0d62d` |
| [`pkg/schedule/schedulers/task_068_test.rs`](../../../pkg/schedule/schedulers/task_068_test.rs) | 95 | `42d1d57a9f1010dfebeba6f16039d9d671ef7afe205b5acc763239318019091e` |
| [`pkg/schedule/schedulers/task_069_test.rs`](../../../pkg/schedule/schedulers/task_069_test.rs) | 95 | `3ec81b093df222a86b4804f59edb85b5a2906baf7e7c6571a33fac3622b662bd` |
| [`pkg/schedule/schedulers/task_070_test.rs`](../../../pkg/schedule/schedulers/task_070_test.rs) | 95 | `4229ec0aa7a5d694f2270c106caf04ad3a388293c17e08a88eed728fcccc4755` |
| [`pkg/schedule/schedulers/task_071_test.rs`](../../../pkg/schedule/schedulers/task_071_test.rs) | 95 | `ad38f2938caaf7fb6dc954938b7cdc5317de613058f2b345eafa78d0404aa0b7` |
| [`pkg/schedule/schedulers/task_072_test.rs`](../../../pkg/schedule/schedulers/task_072_test.rs) | 95 | `0cdc50fdef1cd08b88d2b3eb8a30c4945d624c452761224355cbc76e0d97eb13` |
| [`pkg/schedule/schedulers/task_073_test.rs`](../../../pkg/schedule/schedulers/task_073_test.rs) | 95 | `aa483b23b74d44d8f59eaf7e53f296f078bb994798f6d47184288af320b0f024` |
| [`pkg/schedule/schedulers/task_074_test.rs`](../../../pkg/schedule/schedulers/task_074_test.rs) | 95 | `ddaa032dda9f2a288461708f81160275181b04659bec14823886c39333d66414` |
| [`pkg/schedule/schedulers/task_075_test.rs`](../../../pkg/schedule/schedulers/task_075_test.rs) | 95 | `4242bb3c124a0f380db0c35fd255d14ef37f5bcba53d19d34adb906c90c3bf58` |
| [`pkg/schedule/schedulers/task_076_test.rs`](../../../pkg/schedule/schedulers/task_076_test.rs) | 95 | `9e268b60498b9669c98f3ec24064ba87c1fc7b601bc9472aeb6fa11ff537c426` |
| [`pkg/schedule/schedulers/task_077_test.rs`](../../../pkg/schedule/schedulers/task_077_test.rs) | 95 | `c85393eb804ceb4f8f82b7c9028284355e0fb54464d40de12d5d12f5bfec4808` |
| [`pkg/schedule/schedulers/task_078_test.rs`](../../../pkg/schedule/schedulers/task_078_test.rs) | 95 | `8ff2d2948aa76164e297fd441afa6214e9858308ccecc4e5db784ae6fd0ebeb0` |
| [`pkg/schedule/schedulers/task_079_test.rs`](../../../pkg/schedule/schedulers/task_079_test.rs) | 95 | `ce96663f5cb31f5a1343d92f48741410778c946fd82863083213cb622cdb836e` |
| [`pkg/schedule/schedulers/task_080_test.rs`](../../../pkg/schedule/schedulers/task_080_test.rs) | 95 | `edb680ed30a256e587791b78d7a17800441660932af0f8fb2b24d7c7b1a4e08a` |
| [`pkg/schedule/schedulers/task_081_test.rs`](../../../pkg/schedule/schedulers/task_081_test.rs) | 95 | `8745222fb6bec498559e0dda6dc37cb834d96db23e3f8fa97359a75b5a2c27c1` |
| [`pkg/schedule/schedulers/task_082_test.rs`](../../../pkg/schedule/schedulers/task_082_test.rs) | 95 | `dc84b2a22883ef8520311836b9a2d45bb0c4199ab014881c9d1481ba19cc456c` |
| [`pkg/schedule/schedulers/task_083_test.rs`](../../../pkg/schedule/schedulers/task_083_test.rs) | 95 | `ffbe45ea673beed35e12ed320f5cbfa04ff2638b997a230b762900d6ae982620` |
| [`pkg/schedule/schedulers/task_084_test.rs`](../../../pkg/schedule/schedulers/task_084_test.rs) | 95 | `7266a2412b88bdadfb4d8f09dea7fdcfac20d6e90685c8da37ce4dc4cebf035c` |
| [`pkg/schedule/schedulers/task_085_test.rs`](../../../pkg/schedule/schedulers/task_085_test.rs) | 95 | `002794f77e3987b8ab83b90f6b522baa52d196b5df64233555216d547f3f1a2f` |
| [`pkg/schedule/schedulers/task_086_test.rs`](../../../pkg/schedule/schedulers/task_086_test.rs) | 95 | `1668b1723505942224cfb37799a71160e283095900fc568b55edb237448169d1` |
| [`pkg/schedule/schedulers/task_087_test.rs`](../../../pkg/schedule/schedulers/task_087_test.rs) | 95 | `454145358d9cfe2d8c9464bb152538f7e28f1d2b4a22a20a9416b06ff41cce11` |
| [`pkg/schedule/schedulers/task_088_test.rs`](../../../pkg/schedule/schedulers/task_088_test.rs) | 95 | `aa5065b2f25adea3982f219c811888c7f0b496bb65a22776aaa1e94805026254` |
| [`pkg/schedule/schedulers/task_089_test.rs`](../../../pkg/schedule/schedulers/task_089_test.rs) | 95 | `384880d219e9956480fff5cbae6a58c21e82aac82089968fd49c3c8f5a06be56` |
| [`pkg/schedule/schedulers/task_090_test.rs`](../../../pkg/schedule/schedulers/task_090_test.rs) | 95 | `aefc7709e82bb150279571be6d012d3e1205176ed0007ab8e7b3133b6d6e331b` |
| [`pkg/schedule/schedulers/task_091_test.rs`](../../../pkg/schedule/schedulers/task_091_test.rs) | 95 | `4014169bdb7554ebcdac92927600a3eba5ca4933ded03a0c8f0ef642acd09e79` |
| [`pkg/schedule/schedulers/task_092_test.rs`](../../../pkg/schedule/schedulers/task_092_test.rs) | 95 | `9b93c254612ea908a19c29ac5bc679c4ba9d8fa78c8bb5a5d0ec704298d88f34` |
| [`pkg/schedule/schedulers/task_093_test.rs`](../../../pkg/schedule/schedulers/task_093_test.rs) | 95 | `ee86079d0a304e23c0c39f9317f35c60e8ac6ee67c6396f3deb831bd388742bf` |
| [`pkg/schedule/schedulers/task_094_test.rs`](../../../pkg/schedule/schedulers/task_094_test.rs) | 95 | `bc53c708bdef1cdc655b8d900b6f119d60ffaa71d51f640cdb18b44226730db5` |
| [`pkg/schedule/schedulers/task_095_test.rs`](../../../pkg/schedule/schedulers/task_095_test.rs) | 95 | `365c8107f76af57bb454149f1a78a7db489b77b759ea08cd33c33f707dc854dc` |
| [`pkg/schedule/schedulers/task_096_test.rs`](../../../pkg/schedule/schedulers/task_096_test.rs) | 95 | `41c101a42ce4a17ed54b81db5f507b7b01423f7e0f68ff987a8d3615b2f71af0` |
| [`pkg/schedule/schedulers/task_097_test.rs`](../../../pkg/schedule/schedulers/task_097_test.rs) | 95 | `96d3f02d03cfffb51738735aeaead2bf2057a5f96f9af6ce198738f2c01374c8` |
| [`pkg/schedule/schedulers/task_098_test.rs`](../../../pkg/schedule/schedulers/task_098_test.rs) | 95 | `986cac6cea05f8bc9edab4da888def96bcc95c2d6dcc48527f7a2ecd70f5d96e` |
| [`pkg/schedule/schedulers/task_099_test.rs`](../../../pkg/schedule/schedulers/task_099_test.rs) | 95 | `128e7ca7db3c0c20fe8363728d312086da3af3eaee3c01094c2c978c32995604` |
| [`pkg/schedule/schedulers/task_100_test.rs`](../../../pkg/schedule/schedulers/task_100_test.rs) | 95 | `f8189570dddc5d4e077f961a1b7ee2e062e9fd49b1e35824d7bc73ead162b183` |
| [`pkg/schedule/schedulers/task_101_test.rs`](../../../pkg/schedule/schedulers/task_101_test.rs) | 95 | `d0e0fc175e6e0212344c83a72bbaa96f9db71000744cdb4c2fff8e8c8f595f6c` |
| [`pkg/schedule/schedulers/task_102_test.rs`](../../../pkg/schedule/schedulers/task_102_test.rs) | 95 | `7f04e165e6778018339304c8d43472bf2182aa716d5d7c3cc78550cb60505f10` |
| [`pkg/schedule/schedulers/task_103_test.rs`](../../../pkg/schedule/schedulers/task_103_test.rs) | 95 | `fe3d56a8c3db48ee7d52c637424ba419be000b9d01c5c9529b8de50367b5f18a` |
| [`pkg/schedule/schedulers/task_104_test.rs`](../../../pkg/schedule/schedulers/task_104_test.rs) | 95 | `242f66c65838b6abb9f195a25c84d626a38e5b34f3d861e44fff4cf7b5277704` |
| [`pkg/schedule/schedulers/task_105_test.rs`](../../../pkg/schedule/schedulers/task_105_test.rs) | 95 | `7e374a2baef916093fe18ec6ccdc4e193e529ca514caf27a227b644e1b6ad050` |
| [`pkg/schedule/schedulers/task_106_test.rs`](../../../pkg/schedule/schedulers/task_106_test.rs) | 95 | `d7312f6b28d8d841a55df74493d39390ec3aaa68df58bfaed1dc93eb241b2dda` |
| [`pkg/schedule/schedulers/task_107_test.rs`](../../../pkg/schedule/schedulers/task_107_test.rs) | 95 | `5e29b074b071aae0778c0b709ba5ca670ad6151e64e295dfca197aea7d62ca09` |
| [`pkg/schedule/schedulers/task_108_test.rs`](../../../pkg/schedule/schedulers/task_108_test.rs) | 95 | `e59bfcc4454c87d7775a5742b6feff45c6c4d5822b0ecd0ee8dacddc634bc824` |
| [`pkg/schedule/schedulers/task_109_test.rs`](../../../pkg/schedule/schedulers/task_109_test.rs) | 95 | `ba163d1c7422721b008eca753965a6728ea9fc490f5b9f12137add66afb1ea57` |
| [`pkg/schedule/schedulers/task_110_test.rs`](../../../pkg/schedule/schedulers/task_110_test.rs) | 95 | `5cb08465f8a396aede72ffb438e01868fa3972bee61adca49cc5e230b3e647f6` |
| [`pkg/schedule/schedulers/transfer_witness.rs`](../../../pkg/schedule/schedulers/transfer_witness.rs) | 95 | `9ad92063c8a60da1948fcc9e3dddeac4b2975693511762e99ab894dd553dccc4` |
| [`pkg/schedule/schedulers/transfer_witness.rs.md`](../../../pkg/schedule/schedulers/transfer_witness.rs.md) | 95 | `ba5c4b2efb59dce981e73d30cbcfeed306287090687d724660586fd9d9754840` |
| [`pkg/schedule/schedulers/transfer_witness_leader.go`](../../../pkg/schedule/schedulers/transfer_witness_leader.go) | 95 | `b89a92588c33b70a13c885f53410b0103e65fffb1b8176d5b1d65418248a68e3` |
| [`pkg/schedule/schedulers/transfer_witness_leader_test.go`](../../../pkg/schedule/schedulers/transfer_witness_leader_test.go) | 95 | `39d4ab013057c82c87deb84d07ef4a46518f692baebb47cfd00a874efb085eca` |
| [`pkg/schedule/schedulers/utils.go`](../../../pkg/schedule/schedulers/utils.go) | 95 | `41a551f1b4f95e31a0f48cfc262a125e1132dfa546a73a9300b8eb7bd18834e1` |
| [`pkg/schedule/schedulers/utils_test.go`](../../../pkg/schedule/schedulers/utils_test.go) | 95 | `6d52b31292c27b6f5659d133635726c112bdbbd4df1c0de2d4388f76abe14889` |
| [`pkg/schedule/splitter/Cargo.toml`](../../../pkg/schedule/splitter/Cargo.toml) | 30,95 | `cc1d9407dbf0b0ab70f0a77a7c0611d92af140e8c2638b03b631899627a0f3d3` |
| [`pkg/schedule/splitter/lib.rs`](../../../pkg/schedule/splitter/lib.rs) | 30,95 | `d83443b0e707dc5bfd5846ad0b54f242fa74e1796d3dba6c10427d7350887cf5` |
| [`pkg/schedule/splitter/region_splitter.go`](../../../pkg/schedule/splitter/region_splitter.go) | 30,95 | `6b940638e508d361468417bd8612b2589637c1fe7d79f96ce0a0c65ecfc7681d` |
| [`pkg/schedule/splitter/region_splitter_test.go`](../../../pkg/schedule/splitter/region_splitter_test.go) | 30,95 | `a1d95e543bbd34251f0d60bdfb551dedf6bfb9b91d58d4ff9a721f0036a4256d` |
| [`pkg/schedule/splitter/split_keys.rs`](../../../pkg/schedule/splitter/split_keys.rs) | 30,95 | `833fae46f9a17a4100106aac0b92bf816c4561bb527dfb8b9028d4ebc1a83ad1` |
| [`pkg/schedule/splitter/split_keys.rs.md`](../../../pkg/schedule/splitter/split_keys.rs.md) | 30,95 | `1b8b5b68b4fa63ee16b1de132080d7c6ff27ebe643661a615cbf309cb7b7c982` |
| [`pkg/schedule/splitter/split_results.rs`](../../../pkg/schedule/splitter/split_results.rs) | 30,95 | `05285959280e44ec2d0cba089bcef935c1cce46f535b7e156acbcd65a3604c0c` |
| [`pkg/schedule/splitter/split_results.rs.md`](../../../pkg/schedule/splitter/split_results.rs.md) | 30,95 | `f712f6cffa32484d7d2b7dfa8304821ee468a80ac7721bfbc0edccfb17e9aba5` |
| [`pkg/schedule/splitter/split_retry.rs`](../../../pkg/schedule/splitter/split_retry.rs) | 30,95 | `e225af84b708fd5ac9260ae26bc8f63cd20d8e14630d5810a43a88594001634b` |
| [`pkg/schedule/splitter/split_retry.rs.md`](../../../pkg/schedule/splitter/split_retry.rs.md) | 30,95 | `ee35793234ece7a861efb3f8607af3f88d825f34912a9d313e0dfb4ca0cfff97` |
| [`pkg/schedule/splitter/task_062_test.rs`](../../../pkg/schedule/splitter/task_062_test.rs) | 30,95 | `f208b38fb2ca2b0c2b4490385a54a7ad64485552b4b2d021ddefde4f743f93b2` |
| [`pkg/schedule/splitter/task_063_test.rs`](../../../pkg/schedule/splitter/task_063_test.rs) | 30,95 | `3356e1f308db0bfd0ce6ae44b261780fa7b5844b2d9aa7272c53d37ed3d3ef2b` |
| [`pkg/schedule/splitter/task_064_test.rs`](../../../pkg/schedule/splitter/task_064_test.rs) | 30,95 | `63cec95c54c1665c5c9f001369bded43a34dd6026154e816e34c03a51b93835e` |
| [`pkg/schedule/task_112_test.rs`](../../../pkg/schedule/task_112_test.rs) | 95 | `37c9bfc62b40694e87ca4e13bd8744b78c3d29461d32cdca8cd3a36151d046ad` |
| [`pkg/schedule/task_113_test.rs`](../../../pkg/schedule/task_113_test.rs) | 95 | `361a4e44bb1c3d183e9c7c1e4e0c52884d8292afad46808e1ba2d4cda0b27d32` |
| [`pkg/schedule/task_114_test.rs`](../../../pkg/schedule/task_114_test.rs) | 95 | `d47ddef2c51526f21de33e3160c377019373c5dc8f26f0a3a6c5be1f01bf91c7` |
| [`pkg/schedule/task_115_test.rs`](../../../pkg/schedule/task_115_test.rs) | 95 | `eeea54cbb214661fff0015436e15e704302e0c6b482a8fc9c620edd5ce78b8d6` |
| [`pkg/schedule/task_116_test.rs`](../../../pkg/schedule/task_116_test.rs) | 95 | `ff9377451554fadec26e6c73f10768b19d5152c25214792bd9f7248e0182784f` |
| [`pkg/schedule/task_117_test.rs`](../../../pkg/schedule/task_117_test.rs) | 95 | `bdd3067a8836d2303470710dac18c8588c5996ad3dd5878ba009e02e7de605af` |
| [`pkg/schedule/types/Cargo.toml`](../../../pkg/schedule/types/Cargo.toml) | 95 | `733f48cd046e71f5ece45c9fb36b9e707243d9900d75a196dc20cb1f0037e06c` |
| [`pkg/schedule/types/fixtures/classic.json`](../../../pkg/schedule/types/fixtures/classic.json) | 95 | `c69f72f6fa3d6e3f142a8042f45272d3ff0d6f329a213467b50dc570af35a4c5` |
| [`pkg/schedule/types/fixtures/classic.out`](../../../pkg/schedule/types/fixtures/classic.out) | 95 | `776779e9196edf6485ad565700863d664c965f08c69d9ab236d11544a924b89e` |
| [`pkg/schedule/types/fixtures/nextgen.json`](../../../pkg/schedule/types/fixtures/nextgen.json) | 95 | `69580459fa7547aba0a292b2b98608b4ebb26b26f069fea681814fd7c238c44d` |
| [`pkg/schedule/types/fixtures/nextgen.out`](../../../pkg/schedule/types/fixtures/nextgen.out) | 95 | `22fa62c87c6da100c867d17dcaa702dea927e8b8a7a3d6bc13f462a506ca48b5` |
| [`pkg/schedule/types/fixtures/string-classic.out`](../../../pkg/schedule/types/fixtures/string-classic.out) | 95 | `1111c9417b5f510ab8b0f9e84052c0f08448ee704477bbd7b4164146e791d3cc` |
| [`pkg/schedule/types/fixtures/string-nextgen.out`](../../../pkg/schedule/types/fixtures/string-nextgen.out) | 95 | `40e2b62985f5b21815b259ca912f042705d85e6bb5fff03a432fb3254a96f3f3` |
| [`pkg/schedule/types/lib.rs`](../../../pkg/schedule/types/lib.rs) | 95 | `efeab753ad65b0040169e95f719fdc41b8365095f169dd943a40ffe5abe96428` |
| [`pkg/schedule/types/lib.rs.md`](../../../pkg/schedule/types/lib.rs.md) | 95 | `769724c875cb61068528bb878560990ee7cd8b96dcc1893c24dc78c417cd0330` |
| [`pkg/schedule/types/type.go`](../../../pkg/schedule/types/type.go) | 95 | `b7bc852de78e9a3ce6c06ef98cfd05c36d993e8dbcd2b074c9ec07c0ce0b49bb` |
| [`pkg/schedule/types/types.rs`](../../../pkg/schedule/types/types.rs) | 95 | `1835b12a9ca41a19d43f863b439999df3cd39a5e02fd7ca16d31332093b50d9e` |
| [`pkg/schedule/types/types.rs.md`](../../../pkg/schedule/types/types.rs.md) | 95 | `767a5e70d8d4b32b949eb4b9f3b292feab3eba7c67ab4879804dd4aaed8ebbfb` |
| [`pkg/schedule/types/types_contract_test.rs`](../../../pkg/schedule/types/types_contract_test.rs) | 95 | `1cd3efeb392da913ce19d0e8876d51ce4a1a0c6d2bef7416bb6df810f6f5351b` |
| [`pkg/server/Cargo.toml`](../../../pkg/server/Cargo.toml) | 3 | `c84d7de24016c11f26da6cf0d2a16e701c878045dfc67923827bbc6f20849fa2` |
| [`pkg/server/bucket_heartbeat_adapter.rs`](../../../pkg/server/bucket_heartbeat_adapter.rs) | 3,10 | `bc90b608e30374cd413b37545ad7bd01044838ec662643f274f55ab27d098ece` |
| [`pkg/server/bucket_heartbeat_adapter.rs.md`](../../../pkg/server/bucket_heartbeat_adapter.rs.md) | 3 | `27081dcc01d97bdb3aa22fcdc322ba7724622dadf7ef14993bb19e85b6696974` |
| [`pkg/server/bucket_heartbeat_adapter_test.rs`](../../../pkg/server/bucket_heartbeat_adapter_test.rs) | 3 | `a48b737721749b4410756b1c8290821a105ac1bfe02e0cf69c9d75e8b7724d69` |
| [`pkg/server/core_region_adapter.rs`](../../../pkg/server/core_region_adapter.rs) | 3,23 | `a38004d49dfad69b02c81d90e6ed98ec166b6e455f464eaa9a05dc3f9becb2a2` |
| [`pkg/server/core_region_adapter_test.rs`](../../../pkg/server/core_region_adapter_test.rs) | 3 | `ddf35f5cf6f65741858c55a8584fa1cc87ab47174a51996135d77699ff18ebf7` |
| [`pkg/server/core_store_adapter.rs`](../../../pkg/server/core_store_adapter.rs) | 3,22 | `226822ece326cb81dec448e9297e466c1b538b1a0c342929a6732e3e1ee015b6` |
| [`pkg/server/core_store_adapter_test.rs`](../../../pkg/server/core_store_adapter_test.rs) | 3 | `cf7a5538d755e239452229a399003b3bf59f18bf35fadb93ac45e8ae8155ec2a` |
| [`pkg/server/core_trace.rs`](../../../pkg/server/core_trace.rs) | 3 | `5f16f6064816bcf7895ce7a8cfc588d69c9c3121cfd79e12d93041d03c5aaf47` |
| [`pkg/server/core_trace_test.rs`](../../../pkg/server/core_trace_test.rs) | 3 | `88c81bbb3d7a7909290cdb9e91326253f038e80ea34249aa847df1ecfbf1ada5` |
| [`pkg/server/examples/bucket_heartbeat_oracle.rs`](../../../pkg/server/examples/bucket_heartbeat_oracle.rs) | 3 | `4593bb434c2490c715dbb45d453d08f91d9561435122def4b93b75ba5569454e` |
| [`pkg/server/examples/bucket_heartbeat_oracle.rs.md`](../../../pkg/server/examples/bucket_heartbeat_oracle.rs.md) | 3 | `2781fcfb346f351630319b2ed8649c454a94801539eb8cbb0d0732d8bc8d4ada` |
| [`pkg/server/examples/region_heartbeat_oracle.rs`](../../../pkg/server/examples/region_heartbeat_oracle.rs) | 3 | `d084ae065a8b2048a7a5491af248028f8d57c1f42dd05131808501150c01190c` |
| [`pkg/server/examples/region_heartbeat_oracle.rs.md`](../../../pkg/server/examples/region_heartbeat_oracle.rs.md) | 3 | `ec99243396a5adedbe4316fa5e4048113d909c83435c015e3f43c0f97d1b47db` |
| [`pkg/server/examples/store_heartbeat_oracle.rs`](../../../pkg/server/examples/store_heartbeat_oracle.rs) | 3 | `3ab50cccdf9e9c2baab8fcc2d3ca7101d6786f3e4171ec7a6c5d15c0178aad37` |
| [`pkg/server/examples/store_heartbeat_oracle.rs.md`](../../../pkg/server/examples/store_heartbeat_oracle.rs.md) | 3 | `5c3559d859e95b437372759ac8f188c6f5bc5e80067481b57903de9869cc460f` |
| [`pkg/server/examples/sync_codec_oracle.rs`](../../../pkg/server/examples/sync_codec_oracle.rs) | 3 | `96db3f6370dfed3584e2fae59054d604ee8cd501c0060748565339518ae3285e` |
| [`pkg/server/grpc.rs`](../../../pkg/server/grpc.rs) | 3,18,25,36 | `1505f261fe1b13f3e1db3e0b26d3370f2768304f05d9bd2dd1df054baf0349be` |
| [`pkg/server/grpc.rs.md`](../../../pkg/server/grpc.rs.md) | 3 | `1c1ab9d962ec0e5adc01c4bda75d58bd865730661c0a0eabb4992cdd5e60fb43` |
| [`pkg/server/http.rs`](../../../pkg/server/http.rs) | 3,38,87 | `fb4a46fbfd0bbf03d4e16bb5e37240f006e03c66057b1b6acb2039b6ab87f4f0` |
| [`pkg/server/lib.rs`](../../../pkg/server/lib.rs) | 1,3,5,87 | `81fbddb1ab8ba08c28ef1396628315d3f72bf303fa5c02ddf65901248c906a65` |
| [`pkg/server/lib.rs.md`](../../../pkg/server/lib.rs.md) | 3 | `c952b70d8b1cf6bf4ca573dceea22889e7be8469c752c049fc35745c2c765645` |
| [`pkg/server/region_heartbeat_adapter.rs`](../../../pkg/server/region_heartbeat_adapter.rs) | 3,9 | `2638486b1c1d8e787baf3d91a1570ad894a17095a3214dd8fe758ff129943cff` |
| [`pkg/server/region_heartbeat_adapter.rs.md`](../../../pkg/server/region_heartbeat_adapter.rs.md) | 3 | `32050ec702674428bb13c71bedeb353684223d44e6536e3c077b0b1ee62b54fb` |
| [`pkg/server/region_heartbeat_adapter_test.rs`](../../../pkg/server/region_heartbeat_adapter_test.rs) | 3 | `b4838859365b6ab94a86ca2388b8ca554b2619a94600b8fa258d1623e9ed6b58` |
| [`pkg/server/schedule_admin_balance_leader.rs`](../../../pkg/server/schedule_admin_balance_leader.rs) | 3,65 | `97f04873c66f6a6e76a9f35367eb361f084d59f2db21df194e671b50065b72a1` |
| [`pkg/server/schedule_admin_balance_leader.rs.md`](../../../pkg/server/schedule_admin_balance_leader.rs.md) | 3 | `40823d06e6dabf3030adfb6517df01d3dd02485c03389dadb74a73eaf640517e` |
| [`pkg/server/schedule_admin_balance_leader_test.rs`](../../../pkg/server/schedule_admin_balance_leader_test.rs) | 3 | `d2d0a18a2998117605eea2bcfedece77af62bb1b063350d1b4ae7d4e53dded47` |
| [`pkg/server/schedule_admin_balance_range.rs`](../../../pkg/server/schedule_admin_balance_range.rs) | 3,65 | `0983508a73b89a919c7e8fcd1f30e31338122c9032670300c5573cac684c083d` |
| [`pkg/server/schedule_admin_balance_range.rs.md`](../../../pkg/server/schedule_admin_balance_range.rs.md) | 3 | `782a254663ab5197b546ce34aef9ff2b227583604ad7f0cf6aaebb5b98d5a6f6` |
| [`pkg/server/schedule_admin_balance_range_test.rs`](../../../pkg/server/schedule_admin_balance_range_test.rs) | 3 | `496e238a951593e2ae6260544b31acdad43627c758135660ba59fb9171aecfda` |
| [`pkg/server/schedule_admin_balance_region.rs`](../../../pkg/server/schedule_admin_balance_region.rs) | 3 | `77d6951edc3c7a8ec486e2cda1b5295f38b9b8b594b5231807b72ad74f3e79a2` |
| [`pkg/server/schedule_admin_balance_region.rs.md`](../../../pkg/server/schedule_admin_balance_region.rs.md) | 3 | `74bed19238924f198c722ad73829915fd3d0fea4225c74707caf6d217bbfe705` |
| [`pkg/server/schedule_admin_balance_region_test.rs`](../../../pkg/server/schedule_admin_balance_region_test.rs) | 3 | `e478ccf69ce0effdc5e332fa4ded74610627b5d556ba38943f9ce7feeaa21e09` |
| [`pkg/server/schedule_admin_balance_witness.rs`](../../../pkg/server/schedule_admin_balance_witness.rs) | 3 | `78ef201e39cc7dab8da748219af42a609ef760e32063b6717021e54b9c46caa1` |
| [`pkg/server/schedule_admin_balance_witness.rs.md`](../../../pkg/server/schedule_admin_balance_witness.rs.md) | 3 | `23da9a114d2697afed993527e592b539c92c6daf83b83cfcfd7d574a3e78a017` |
| [`pkg/server/schedule_admin_balance_witness_test.rs`](../../../pkg/server/schedule_admin_balance_witness_test.rs) | 3 | `ccd9694afbef84aeff8f4dd09c8c25155a80a17c4fc896fea16b576f1f925136` |
| [`pkg/server/schedule_admin_evict_leader.rs`](../../../pkg/server/schedule_admin_evict_leader.rs) | 3 | `bce4297042b781fc9042c4a0098ecff1e0c1f860a1bf708f4964ad15f41ff881` |
| [`pkg/server/schedule_admin_evict_leader.rs.md`](../../../pkg/server/schedule_admin_evict_leader.rs.md) | 3 | `baf3b18957f191d02ebd8a5e956bb5a8da897ad45b77f94b3dfc957b01c1ea0e` |
| [`pkg/server/schedule_admin_evict_leader_test.rs`](../../../pkg/server/schedule_admin_evict_leader_test.rs) | 3 | `4032a4e38b4a94dab9d8f1238984b1fc9561200150dd2179c58a9ceee77e7ad2` |
| [`pkg/server/schedule_admin_evict_slow_store.rs`](../../../pkg/server/schedule_admin_evict_slow_store.rs) | 3 | `7208f790fad199e74a1a02ecb69d4ab6122ac7917977e2725e7cd1ff5be8191b` |
| [`pkg/server/schedule_admin_evict_slow_store.rs.md`](../../../pkg/server/schedule_admin_evict_slow_store.rs.md) | 3 | `4fc77d6df8784d91e884694e6f1360c5d96b9d781127e37b84a2ea2c2300516a` |
| [`pkg/server/schedule_admin_evict_slow_store_test.rs`](../../../pkg/server/schedule_admin_evict_slow_store_test.rs) | 3 | `f8623650b0a188bb6861fb7f07249231d3654893dde4dae5d85de4e3428421d2` |
| [`pkg/server/schedule_admin_evict_slow_trend.rs`](../../../pkg/server/schedule_admin_evict_slow_trend.rs) | 3 | `7df8bbb80292dfab757f6dd45b517a2e64b76936411b461fc0891f51a3ffa6bd` |
| [`pkg/server/schedule_admin_evict_slow_trend.rs.md`](../../../pkg/server/schedule_admin_evict_slow_trend.rs.md) | 3 | `03635a2d108e3a806dba7916ac0faf01779d3b9b034ad6ad7cd9310038c262e4` |
| [`pkg/server/schedule_admin_evict_slow_trend_test.rs`](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs) | 3 | `3bc52bbb9e3bcd9c49eddec7a81fd8d8800f4df8288f6c18434c461b6ecf1c34` |
| [`pkg/server/schedule_admin_evict_stopping_store.rs`](../../../pkg/server/schedule_admin_evict_stopping_store.rs) | 3 | `4a6cabab8bfcea01d4ab88464ca0f65491a29e14c9a531f80d828f5eb66475ca` |
| [`pkg/server/schedule_admin_evict_stopping_store.rs.md`](../../../pkg/server/schedule_admin_evict_stopping_store.rs.md) | 3 | `49eb7e6e662b8e23241950566920734312115db1ed02ada0de390a706776bcc8` |
| [`pkg/server/schedule_admin_evict_stopping_store_test.rs`](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs) | 3 | `7397f09336f5599a8586b48acdb45413ed1b742cf90f2e3c720c31328f0dbb03` |
| [`pkg/server/schedule_admin_grant_hot_region.rs`](../../../pkg/server/schedule_admin_grant_hot_region.rs) | 3 | `0b54ac12b221366685d8db47c4e1f27346359219eca2550f274ffda197c9f485` |
| [`pkg/server/schedule_admin_grant_hot_region.rs.md`](../../../pkg/server/schedule_admin_grant_hot_region.rs.md) | 3 | `33ff380b4c81bcb82ebdb6b6016fa86ae54f2208ba66d50c2451dfeb15b0897a` |
| [`pkg/server/schedule_admin_grant_hot_region_test.rs`](../../../pkg/server/schedule_admin_grant_hot_region_test.rs) | 3 | `90617574a262a2a34e9f42c14f23eab294a8f1a0fbb9408002c04ab4bc7ae753` |
| [`pkg/server/schedule_admin_grant_leader.rs`](../../../pkg/server/schedule_admin_grant_leader.rs) | 3 | `f1967b9a9bad1f20e8209debe17a71485a47b3eef51d24d25bf20e75699f2c26` |
| [`pkg/server/schedule_admin_grant_leader.rs.md`](../../../pkg/server/schedule_admin_grant_leader.rs.md) | 3 | `e9f61bbcc69e526946d52f7d9b01a4cd3aa831510da1058632cb113304d502ae` |
| [`pkg/server/schedule_admin_grant_leader_test.rs`](../../../pkg/server/schedule_admin_grant_leader_test.rs) | 3 | `9defb8e4dbbfd0f6b4d6fcf4e8c0984ae6b8fba740b1a85aded8d5384393e88a` |
| [`pkg/server/schedule_admin_hot_region.rs`](../../../pkg/server/schedule_admin_hot_region.rs) | 3 | `3507173745338790da728951756880a5d2146d2c9d161b944d3bfe760217e087` |
| [`pkg/server/schedule_admin_hot_region.rs.md`](../../../pkg/server/schedule_admin_hot_region.rs.md) | 3 | `b654adf5768707b2e4d2a5459b74dce418dfcd5d111e678bb24506bf286cd4d2` |
| [`pkg/server/schedule_admin_hot_region_test.rs`](../../../pkg/server/schedule_admin_hot_region_test.rs) | 3 | `02b9ea9dcf6ce49a276637a19bdbeec63a429a78f2cdf3cefcfacde20be91108` |
| [`pkg/server/schedule_admin_label.rs`](../../../pkg/server/schedule_admin_label.rs) | 3 | `2e28d6a52a30f79bd5899478acd2153a45cbf69b906fc28278f2a3f6198da8cc` |
| [`pkg/server/schedule_admin_label.rs.md`](../../../pkg/server/schedule_admin_label.rs.md) | 3 | `9f3514baac7589cfd2641e60edb0ea090a30c60781a5a52b0150efa976f175fd` |
| [`pkg/server/schedule_admin_label_test.rs`](../../../pkg/server/schedule_admin_label_test.rs) | 3 | `20ca4a4170e1530b1c11cf95e1667f5ac0281a147077ad51237cb02916dbc829` |
| [`pkg/server/schedule_admin_random_merge.rs`](../../../pkg/server/schedule_admin_random_merge.rs) | 3 | `0efd659bceba0e828c1ff14081644418ed8b0a14464de9c549f98f691663549d` |
| [`pkg/server/schedule_admin_random_merge.rs.md`](../../../pkg/server/schedule_admin_random_merge.rs.md) | 3 | `ca2e420e081a38226729088289e6eb0aa905b09085447cbf3570ff0f569b00cc` |
| [`pkg/server/schedule_admin_random_merge_test.rs`](../../../pkg/server/schedule_admin_random_merge_test.rs) | 3 | `2f35b585b81a3d325d7e161a30bd81c1d94229f22b5c05847d3cf0e6b08df08f` |
| [`pkg/server/schedule_admin_scatter_range.rs`](../../../pkg/server/schedule_admin_scatter_range.rs) | 3 | `fdb95ffee664c13c3bfa53f53263aa6de9e369f802ad996ff2509592b3dc2946` |
| [`pkg/server/schedule_admin_scatter_range.rs.md`](../../../pkg/server/schedule_admin_scatter_range.rs.md) | 3 | `64e5091d539c91eb2d88903a03e7f334cedf8d7eda3674c18cd0e05e150f5dbe` |
| [`pkg/server/schedule_admin_scatter_range_test.rs`](../../../pkg/server/schedule_admin_scatter_range_test.rs) | 3 | `6e9ef37f534db527bed3bf8b6502fbc9a869b5ab0d6aab32d5f54041ec2729e7` |
| [`pkg/server/schedule_admin_shuffle_hot_region.rs`](../../../pkg/server/schedule_admin_shuffle_hot_region.rs) | 3 | `b9eed12620d7a50662c293273387226199ac3099f6d6bee7e2aab88521ad95f5` |
| [`pkg/server/schedule_admin_shuffle_hot_region.rs.md`](../../../pkg/server/schedule_admin_shuffle_hot_region.rs.md) | 3 | `ff2a90f6adaf847e0599b583448d27c78740d47e1879216d7358c6d08788460e` |
| [`pkg/server/schedule_admin_shuffle_hot_region_test.rs`](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs) | 3 | `85663a4bcbf8dceab6d2c6417a33de84277e30f3fd1f0a53d4660be94d954694` |
| [`pkg/server/schedule_admin_shuffle_leader.rs`](../../../pkg/server/schedule_admin_shuffle_leader.rs) | 3 | `a7aad72e6e31709baaa091c2a608dfc17cfd31b2342dce48fcc62e20e4e6bfbb` |
| [`pkg/server/schedule_admin_shuffle_leader.rs.md`](../../../pkg/server/schedule_admin_shuffle_leader.rs.md) | 3 | `02f0125905b2f01dad4874026c1bbbf2c485f12f11ccb8b7eff6e5ea11eef693` |
| [`pkg/server/schedule_admin_shuffle_leader_test.rs`](../../../pkg/server/schedule_admin_shuffle_leader_test.rs) | 3 | `2099656caf021127f870cb1087ee3346dcff38a65bb8ab9aad6280a9e2da86ae` |
| [`pkg/server/schedule_admin_shuffle_region.rs`](../../../pkg/server/schedule_admin_shuffle_region.rs) | 3 | `a14629880620fe93c77cd4a59c44dd49b888f3507a9c8209c0c0f3a7e457a3e7` |
| [`pkg/server/schedule_admin_shuffle_region.rs.md`](../../../pkg/server/schedule_admin_shuffle_region.rs.md) | 3 | `9dc0cf2184223f5225f50c7fc0ee46d1579ad71b94ed4dbdf2856f78b5483752` |
| [`pkg/server/schedule_admin_shuffle_region_test.rs`](../../../pkg/server/schedule_admin_shuffle_region_test.rs) | 3 | `8775e84439f5ee4b67b8dbbebf6c5823b64fa7b490dab364810275536335b3d3` |
| [`pkg/server/schedule_admin_transfer_witness_leader.rs`](../../../pkg/server/schedule_admin_transfer_witness_leader.rs) | 3 | `a21192731bb1450f24d15d62748c0accfeeb1a8e4f48a38701180882283db63c` |
| [`pkg/server/schedule_admin_transfer_witness_leader.rs.md`](../../../pkg/server/schedule_admin_transfer_witness_leader.rs.md) | 3 | `8e74f5c16f682af7c7d213f12584dcb43d346d11622878a146a11d2c6892426f` |
| [`pkg/server/schedule_admin_transfer_witness_leader_test.rs`](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs) | 3 | `3e4867fbd8a3fdfd5b36d4ebd0c9fe04d83fef493ad13382709294f24c7887a8` |
| [`pkg/server/schedule_wire.rs`](../../../pkg/server/schedule_wire.rs) | 3 | `e4f386ea8947dfcfbb56ae0152c066395b9b7e6e29fe628169896de3578eb3a8` |
| [`pkg/server/schedule_wire.rs.md`](../../../pkg/server/schedule_wire.rs.md) | 3 | `c75118402812361baaa6c0bfaf75db27febe3b2de61554e31fb0f71df4ae0b64` |
| [`pkg/server/schedule_wire_test.rs`](../../../pkg/server/schedule_wire_test.rs) | 3 | `0ada4192ff1fd4368706c33bf29267763f4abd7d668fe808e071868053a64411` |
| [`pkg/server/server_test.rs`](../../../pkg/server/server_test.rs) | 3 | `025e3b581ef436ebdfbabf551ad0a0ea4b5b87518cf72c798ee64900b0a10c08` |
| [`pkg/server/store_heartbeat_adapter.rs`](../../../pkg/server/store_heartbeat_adapter.rs) | 3,8 | `6f4a3beb03958aab571b806028b14964c9347459a5c68ae070c0f0bf2e00bd2e` |
| [`pkg/server/store_heartbeat_adapter.rs.md`](../../../pkg/server/store_heartbeat_adapter.rs.md) | 3 | `672bd3c49d942147c39de15ae320927af2b776ed38aa882f48e7620b04f0a25e` |
| [`pkg/server/store_heartbeat_adapter_test.rs`](../../../pkg/server/store_heartbeat_adapter_test.rs) | 3 | `bc640dd36e955cd64065005719ff8ad5b883007ea601a8312709d95e47dd37eb` |
| [`pkg/server/sync_codec.rs`](../../../pkg/server/sync_codec.rs) | 3 | `b06ebdc360b1cd6ea58857dc999e5769679b7e0d23d964c413a14cd53287ba9f` |
| [`pkg/server/sync_codec.rs.md`](../../../pkg/server/sync_codec.rs.md) | 3 | `fa80bb124054aadfe943fbc9af7955c5028d84c6d9ef4dae00ffb78593d3c022` |
| [`pkg/server/sync_codec_test.rs`](../../../pkg/server/sync_codec_test.rs) | 3 | `f4c6e660d6c48cdc1a99014ea5321e8334c20d3a69d8e155acc7900ae4f35b11` |
| [`pkg/server/sync_transport.rs`](../../../pkg/server/sync_transport.rs) | 3,11 | `23ffb4f5f18061dd926f22c52fa811ccbedbe8ebbfc8a6a5c3dcfa09e6249789` |
| [`pkg/server/sync_transport.rs.md`](../../../pkg/server/sync_transport.rs.md) | 3 | `514eb6513a891f152adea7d97e7578311b093a803555f9e1b4dabbe5f112d51e` |
| [`pkg/server/sync_transport_test.rs`](../../../pkg/server/sync_transport_test.rs) | 3 | `d33c9c7dffb79061384cd73e7206380b1f9fc6f02fd0150b1a3293db40cf44d7` |
| [`pkg/statistics/Cargo.toml`](../../../pkg/statistics/Cargo.toml) | 60,61 | `0b50c145d7ec2a1c5e13d01444d5612581b33b5c70a2c66b3f9891b35b4b1969` |
| [`pkg/statistics/buckets/Cargo.toml`](../../../pkg/statistics/buckets/Cargo.toml) | 60,61 | `16374eae7696a2e90f666d64b68d7589399bc8a7f1ae62f24827704ac3bc520e` |
| [`pkg/statistics/buckets/bucket_degree.rs`](../../../pkg/statistics/buckets/bucket_degree.rs) | 60,61 | `b2fe6e3ca740b46da6b847152a6077fd1704ec02a73b06111f86103618b5aab2` |
| [`pkg/statistics/buckets/bucket_degree.rs.md`](../../../pkg/statistics/buckets/bucket_degree.rs.md) | 60,61 | `2c93a252b5f956cfa3750e2cfd40674415f35e4066767036b86a517cacd53964` |
| [`pkg/statistics/buckets/bucket_degree_test.rs`](../../../pkg/statistics/buckets/bucket_degree_test.rs) | 60,61 | `473bda6df7ef77fc94d9788616da374ce0c736e6783d810efda9ad127b9fc9c9` |
| [`pkg/statistics/buckets/bucket_inherit.rs`](../../../pkg/statistics/buckets/bucket_inherit.rs) | 60,61 | `a96ba017894d98c4c08232eba9d9b76fec0da320883b86d338103e12e27b2937` |
| [`pkg/statistics/buckets/bucket_inherit.rs.md`](../../../pkg/statistics/buckets/bucket_inherit.rs.md) | 60,61 | `284c4d6cff78dfc7b42c7b7f86e44a16fe8eb2d63359bb94973ea4fbf2a22dbf` |
| [`pkg/statistics/buckets/bucket_inherit_test.rs`](../../../pkg/statistics/buckets/bucket_inherit_test.rs) | 60,61 | `0fd96090dcf0fbbc7eb5caef4ebc9a2572b37052e8d48b073a3b7b85ee37e974` |
| [`pkg/statistics/buckets/bucket_range.rs`](../../../pkg/statistics/buckets/bucket_range.rs) | 60,61 | `4b12a6dd51c7a2a3afe620f39cb8067efab044502ddcbae5f4dad48f85d49d35` |
| [`pkg/statistics/buckets/bucket_range.rs.md`](../../../pkg/statistics/buckets/bucket_range.rs.md) | 60,61 | `73bc736e50b06914649d500ad220a76685d273abcaa7d854fd010d3d75abd4c9` |
| [`pkg/statistics/buckets/bucket_range_test.rs`](../../../pkg/statistics/buckets/bucket_range_test.rs) | 60,61 | `514d3d2b35e6d51fff8a230ec0a307a29a3e7981269729563813347f5b830e9e` |
| [`pkg/statistics/buckets/bucket_stat_informer.go`](../../../pkg/statistics/buckets/bucket_stat_informer.go) | 60,61 | `c4ddbc8a19fc0dbb2ca06f9e3157e8b872baaee104477de230c0c773e7f5e2e1` |
| [`pkg/statistics/buckets/bucket_values.rs`](../../../pkg/statistics/buckets/bucket_values.rs) | 60,61 | `440e6d414f8a2e521919f0e1f081a2b892aeba0f65a8af11f4ba93f5e4a26192` |
| [`pkg/statistics/buckets/bucket_values.rs.md`](../../../pkg/statistics/buckets/bucket_values.rs.md) | 60,61 | `ee31c12755dbfd381029076077776dc26504989820fe5822a65cfb6750c2f414` |
| [`pkg/statistics/buckets/bucket_values_test.rs`](../../../pkg/statistics/buckets/bucket_values_test.rs) | 60,61 | `0d688327486db6f55daabc33d937379e16b6e0dfe1962f5190d75b0ffeb9b187` |
| [`pkg/statistics/buckets/bucket_worker.rs`](../../../pkg/statistics/buckets/bucket_worker.rs) | 60,61 | `3b0c2887c43b3253ffe1c9998397e342244a1678ffcb149b2f7a19cdf44b22a8` |
| [`pkg/statistics/buckets/bucket_worker.rs.md`](../../../pkg/statistics/buckets/bucket_worker.rs.md) | 60,61 | `3d5b6aaba7b201ce02ee5594c5beaf8c6d4af8434e91a2c2b80169b763cf4f2a` |
| [`pkg/statistics/buckets/bucket_worker_test.rs`](../../../pkg/statistics/buckets/bucket_worker_test.rs) | 60,61 | `194e676818cb50970fae63043a520df4cde7b32b33a100709586d6aaeca49fe8` |
| [`pkg/statistics/buckets/hot_bucket_cache.go`](../../../pkg/statistics/buckets/hot_bucket_cache.go) | 60,61 | `f3dbef4b0ca2ff6cee1a5b98c19db7a136101fa1673f141503f7d4c33d6720a8` |
| [`pkg/statistics/buckets/hot_bucket_cache_test.go`](../../../pkg/statistics/buckets/hot_bucket_cache_test.go) | 60,61 | `a23e10516b4465da9fe42cd8de881cd4f6608d6a662bbf7bec91f390591f7d40` |
| [`pkg/statistics/buckets/hot_bucket_task.go`](../../../pkg/statistics/buckets/hot_bucket_task.go) | 60,61 | `1c6d1786bbfd014db6bca81fa9590e6ea031c2bbc9cc7a64bbd544266321b703` |
| [`pkg/statistics/buckets/hot_bucket_task_test.go`](../../../pkg/statistics/buckets/hot_bucket_task_test.go) | 60,61 | `68275a309fb85a5dd0882e73dbd01e495b3aed96be2340b8dd709dc7479a6835` |
| [`pkg/statistics/buckets/lib.rs`](../../../pkg/statistics/buckets/lib.rs) | 60,61 | `7129c2b5b3b841069b599d00b03c3e65d49d525cd34b8874f501225a6527cd4e` |
| [`pkg/statistics/buckets/lib.rs.md`](../../../pkg/statistics/buckets/lib.rs.md) | 60,61 | `1e3ac6b4f623e90b1b02a18c6daa13c60c9710a1bfbfb7c7ec4fe77cef6b22b0` |
| [`pkg/statistics/buckets/metric.go`](../../../pkg/statistics/buckets/metric.go) | 60,61 | `5c912821c3c65252df59ad222c35af974cd3b49555897b774902804b3f9911f9` |
| [`pkg/statistics/collector.go`](../../../pkg/statistics/collector.go) | 60,61 | `981166a57f426673fd3faaa1ae908313c41027f0272273fae351373581e0f900` |
| [`pkg/statistics/cpu.go`](../../../pkg/statistics/cpu.go) | 60,61 | `e9bb58dfea098716c717bbf017f04d98167a384da271b2de08df7016b3b87c49` |
| [`pkg/statistics/cpu.rs`](../../../pkg/statistics/cpu.rs) | 60,61 | `2d5e276c509b862b5f3ee9d11c477c6f35337c96207215d9c0ab0cdbfc968cbb` |
| [`pkg/statistics/cpu.rs.md`](../../../pkg/statistics/cpu.rs.md) | 60,61 | `8d1b95f0f78efbd1990bb56cba0d3369b046b5e518337a272b3bdffce8f13579` |
| [`pkg/statistics/cpu_test.go`](../../../pkg/statistics/cpu_test.go) | 60,61 | `542d701b4cca4713a2579b9f233db7733ad8e239fffa5a133f745ea8e5eca213` |
| [`pkg/statistics/cpu_test.rs`](../../../pkg/statistics/cpu_test.rs) | 60,61 | `eada451ddd857f2c698f4d0b1ddb0e47d145997164436c338eeb1f2dc1cc3ade` |
| [`pkg/statistics/engine_load.rs`](../../../pkg/statistics/engine_load.rs) | 60,61 | `427484f937bfe87d85d3e6409e372248abf35c7f708f9234f1db6c7e7db4e4a8` |
| [`pkg/statistics/engine_load.rs.md`](../../../pkg/statistics/engine_load.rs.md) | 60,61 | `e25d904015280f7295d43501982abfa2eb096060a70cc3630282da4ddc2bc2cd` |
| [`pkg/statistics/engine_load_test.rs`](../../../pkg/statistics/engine_load_test.rs) | 60,61 | `594427c026574a5499cb015282abfde3b2811c70fadb0d7192490aeb1b663897` |
| [`pkg/statistics/hot_cache.go`](../../../pkg/statistics/hot_cache.go) | 60,61 | `358f74db30b7daac888397f869fd5b7d44dab832c05cc8fb92f6434ff1e42cc0` |
| [`pkg/statistics/hot_cache_test.go`](../../../pkg/statistics/hot_cache_test.go) | 60,61 | `024b9f0b1c83ebad6fea44926ea6f6623e1ae877b83c1c5e7cf5d204eb613d2f` |
| [`pkg/statistics/hot_expire.rs`](../../../pkg/statistics/hot_expire.rs) | 60,61 | `8f6fcc63f8eb1db6081d4d258d0670157f63bfca7bd00e2ca6b3bba2d51ba8fc` |
| [`pkg/statistics/hot_expire.rs.md`](../../../pkg/statistics/hot_expire.rs.md) | 60,61 | `c6d709254db240bbd1133636e148cafdf5c00c6be5d89db98a9ccd642108977b` |
| [`pkg/statistics/hot_expire_test.rs`](../../../pkg/statistics/hot_expire_test.rs) | 60,61 | `bfa7a273db16c86b02803ab159dd95b42c7e636ac258a9ea89cae37813faf2f0` |
| [`pkg/statistics/hot_inherit.rs`](../../../pkg/statistics/hot_inherit.rs) | 60,61 | `81af02e9d85552332aa9546fd2a5fbd800ab2d486c6c69ed2cee4833f2b19757` |
| [`pkg/statistics/hot_inherit.rs.md`](../../../pkg/statistics/hot_inherit.rs.md) | 60,61 | `f33ab22e7dc9670cbab7db363095fc890b432fe8dd0e8a53e660dca09aa889c5` |
| [`pkg/statistics/hot_inherit_test.rs`](../../../pkg/statistics/hot_inherit_test.rs) | 60,61 | `cffb2a796295e63f8b93646dd9da99885a7292ad06cac04c72ed12a714ed7da8` |
| [`pkg/statistics/hot_peer.go`](../../../pkg/statistics/hot_peer.go) | 60,61 | `817f3cedd3e92d2aed6152e05ed231517b2c3d73163234d67af7fbc03e61a677` |
| [`pkg/statistics/hot_peer.rs`](../../../pkg/statistics/hot_peer.rs) | 60,61 | `1797da6fa45e55252f6ef70b709ac39e8bf75dc4191ef446e2726d43f5427e11` |
| [`pkg/statistics/hot_peer.rs.md`](../../../pkg/statistics/hot_peer.rs.md) | 60,61 | `5befd8138fd26befb0f0d6bc1da08e1d74ce8626bc5a873a8dd861b88fc51a9b` |
| [`pkg/statistics/hot_peer_cache.go`](../../../pkg/statistics/hot_peer_cache.go) | 60,61 | `f1241c874657d572443ac9f5f18e79d52e9d1efc68f530a3503a31fb25d4631d` |
| [`pkg/statistics/hot_peer_cache_test.go`](../../../pkg/statistics/hot_peer_cache_test.go) | 60,61 | `89cc04cd3fd49b918b42afe1167fd954ee75801b7ff67d5a2ffeab8401f2b5dc` |
| [`pkg/statistics/hot_peer_test.go`](../../../pkg/statistics/hot_peer_test.go) | 60,61 | `43f1a5a2f182eb2f09291f92b334b6937e68bc0956fc1849fd0ec4d1081164a1` |
| [`pkg/statistics/hot_peer_test.rs`](../../../pkg/statistics/hot_peer_test.rs) | 60,61 | `d1ae5cbb90f3ea9d3ea4ed0b521b73105413ebcd4fa34b4bfdd9415f70ef56cf` |
| [`pkg/statistics/hot_peer_update.rs`](../../../pkg/statistics/hot_peer_update.rs) | 60,61 | `c1c26d150c636086f29994e20a23ee8972ac76239eeeada18e51d7f0ce061409` |
| [`pkg/statistics/hot_peer_update.rs.md`](../../../pkg/statistics/hot_peer_update.rs.md) | 60,61 | `4d599fd5f89e746b737592e353a12354f44d7c18bb0f2314ef2b31514d0ddacb` |
| [`pkg/statistics/hot_peer_update_test.rs`](../../../pkg/statistics/hot_peer_update_test.rs) | 60,61 | `eb8ab69665abd45bcd2b32f95a8619e3e48f261a7234f9e7251d94f46633e92a` |
| [`pkg/statistics/hot_projection.rs`](../../../pkg/statistics/hot_projection.rs) | 60,61 | `2fa465a2ac6566f4c6fffbf5cb6d99a7b5a001db151f88b761f2f87ea46effac` |
| [`pkg/statistics/hot_projection.rs.md`](../../../pkg/statistics/hot_projection.rs.md) | 60,61 | `a76cc6b3a4457efcef5d40efb6e426d3916fa05a038720fcae106424ff2ffe90` |
| [`pkg/statistics/hot_projection_test.rs`](../../../pkg/statistics/hot_projection_test.rs) | 60,61 | `ad43d15f803e2b02a47ef864cee92ccb9dfc3d04585bfaf4d4fb38c5a525b5b2` |
| [`pkg/statistics/hot_regions_stat.go`](../../../pkg/statistics/hot_regions_stat.go) | 60,61 | `80a892f6038802221bb9a6d3ca9af7ba835e81bff3bb3a32b5c6cb55a5428842` |
| [`pkg/statistics/hot_stat.go`](../../../pkg/statistics/hot_stat.go) | 60,61 | `e3dc743ec670b5f67c27299aa9ec058339ca9bbfe0628530c7dfb52ce492143f` |
| [`pkg/statistics/hot_stat.rs`](../../../pkg/statistics/hot_stat.rs) | 60,61 | `bcaa871cc25b3183e5c07eca22764c99b4653af44f21fc2497940dbc26f78454` |
| [`pkg/statistics/hot_stat.rs.md`](../../../pkg/statistics/hot_stat.rs.md) | 60,61 | `6fd68e48d64088640c409ae571f43592f59b03256c87b8a875a2b1332a0380f2` |
| [`pkg/statistics/hot_stat_test.rs`](../../../pkg/statistics/hot_stat_test.rs) | 60,61 | `04e45baac52fab3665773f9a19d55c8d517eb6d60d8e1004cac38aef79057027` |
| [`pkg/statistics/hot_threshold.rs`](../../../pkg/statistics/hot_threshold.rs) | 60,61 | `50c8f562c7655b07e7dda84f15a1c1f169f0ab7ba4e09aae3d4672609c3c60f5` |
| [`pkg/statistics/hot_threshold.rs.md`](../../../pkg/statistics/hot_threshold.rs.md) | 60,61 | `dcd157874a0a9d899373399fa04f9f77c6fe2b7042fe15234e1155daa260df4b` |
| [`pkg/statistics/hot_threshold_test.rs`](../../../pkg/statistics/hot_threshold_test.rs) | 60,61 | `e4663b577fb3f45a7d57ac9279aeffdb667b18bc2f1e33e3d45d4be6d174678c` |
| [`pkg/statistics/hot_worker.rs`](../../../pkg/statistics/hot_worker.rs) | 60,61 | `61740cfc7506e255835319fb329374175ce298f9b81a89e805a56fdfbbc62661` |
| [`pkg/statistics/hot_worker.rs.md`](../../../pkg/statistics/hot_worker.rs.md) | 60,61 | `624d57475779e1834a3da720a9a4786fb8c52ce726cf1881073008d0525ae454` |
| [`pkg/statistics/hot_worker_test.rs`](../../../pkg/statistics/hot_worker_test.rs) | 60,61 | `95ae9317dece86285d14f0a83ae73a20455606199bd6f003d3e6f4b90d7457f1` |
| [`pkg/statistics/label_statistics.rs`](../../../pkg/statistics/label_statistics.rs) | 60,61 | `74d5acada14c5f47572dc5c736f587bc1d0d4df5af54c99805a99cd4b4a7d2dc` |
| [`pkg/statistics/label_statistics.rs.md`](../../../pkg/statistics/label_statistics.rs.md) | 60,61 | `99868ff71f8dd61e3720645a9907d909eaf83a98af0fa7bc5e31bef2897d689b` |
| [`pkg/statistics/label_statistics_test.rs`](../../../pkg/statistics/label_statistics_test.rs) | 60,61 | `459ae0d505bfc3a04d0a975c5f5fc3e2e547193985cbb65de3c66b87c4e5a897` |
| [`pkg/statistics/lib.rs`](../../../pkg/statistics/lib.rs) | 60,61 | `9d4508fa0af225578b46dde8fda875ed840753be4d69f81d0973758a60082cc6` |
| [`pkg/statistics/lib.rs.md`](../../../pkg/statistics/lib.rs.md) | 60,61 | `0583c79038c02361cbe19b7c43e214c6a48c4b8153a6ef13b8bfddbc3f6fac12` |
| [`pkg/statistics/load_values.rs`](../../../pkg/statistics/load_values.rs) | 60,61 | `7c5bf4b6518d7b075cb722a8034cc9c78c129f97e43133bfb9e671e0b7c120a4` |
| [`pkg/statistics/load_values.rs.md`](../../../pkg/statistics/load_values.rs.md) | 60,61 | `e33475b029a46a74cb355583d7d6e9babea0f613a11f7fcc6d234c7eed4f70cb` |
| [`pkg/statistics/load_values_test.rs`](../../../pkg/statistics/load_values_test.rs) | 60,61 | `c8d9f9b91174e59200ac813921a766484e0f9fd79ebb40c241dc640725281dad` |
| [`pkg/statistics/metrics.go`](../../../pkg/statistics/metrics.go) | 60,61 | `90722435c505b7ea9478fafa0ed0c5e60fd85c26461a0c11b1a10cb6a6e26aa0` |
| [`pkg/statistics/metrics.rs`](../../../pkg/statistics/metrics.rs) | 60,61 | `7583761f8f9ceaa4133e99476146288d04a5f5b16cc8a7e2e019d410ed9cc8d1` |
| [`pkg/statistics/metrics.rs.md`](../../../pkg/statistics/metrics.rs.md) | 60,61 | `4431ef63093d20081f9509a96337ddf7b8a292ed495deb6fe3c44e1bb43a9cbf` |
| [`pkg/statistics/metrics_test.rs`](../../../pkg/statistics/metrics_test.rs) | 60,61 | `9c643522eed7b1ba3a30e646e93ec9aa167576bc2a9cdbb63f97d190c4d94927` |
| [`pkg/statistics/region.go`](../../../pkg/statistics/region.go) | 60,61 | `f6c06c11e4a41a39b55f27091a7d928bae999eb1a293dca70ab4d99aa351d64a` |
| [`pkg/statistics/region_classify.rs`](../../../pkg/statistics/region_classify.rs) | 60,61 | `a3fa1529081df395f68161cec1a7ccaed3587a231999ebcb4a107b4ef60d85cf` |
| [`pkg/statistics/region_classify.rs.md`](../../../pkg/statistics/region_classify.rs.md) | 60,61 | `077239afac612869d44139b11275719a683a24ebba1543ccb5a1cb9394d3e22d` |
| [`pkg/statistics/region_classify_test.rs`](../../../pkg/statistics/region_classify_test.rs) | 60,61 | `0afddda0aa263349fec2fa7e108fd686906535916592c5f6b33c6a2c7de2d823` |
| [`pkg/statistics/region_collection.go`](../../../pkg/statistics/region_collection.go) | 60,61 | `56a19dafd4492a15644ef92c9cd159cbfd51e341f1ce80735ee8f85ebd0e41dc` |
| [`pkg/statistics/region_collection_test.go`](../../../pkg/statistics/region_collection_test.go) | 60,61 | `07498f719728a9df9d5638acff54a93bff6b485ce31037dd0a093b63d736ff17` |
| [`pkg/statistics/region_placement.rs`](../../../pkg/statistics/region_placement.rs) | 60,61 | `c76b637fabdc01daa49b7cb7dd0610a5318167e41364d545a2d08a1e87c4d36f` |
| [`pkg/statistics/region_placement.rs.md`](../../../pkg/statistics/region_placement.rs.md) | 60,61 | `544c0ac55328b3df92c25d535de27c89abfe92ee34a1b6dbb6fcb6517d90e0c8` |
| [`pkg/statistics/region_placement_test.rs`](../../../pkg/statistics/region_placement_test.rs) | 60,61 | `150a01c3774eb774dd1e60ab1ad3fe082f8697d1b559df9a0c761ba94b9f9693` |
| [`pkg/statistics/region_stat_informer.go`](../../../pkg/statistics/region_stat_informer.go) | 60,61 | `8fa28cb473e34dc0050f63f6d69f4a7b997f95077865a13bb7aafdee61a12517` |
| [`pkg/statistics/region_totals.rs`](../../../pkg/statistics/region_totals.rs) | 60,61 | `67f1112b023bfecdab9920714a07ee728d6b3bc339c6ce1f01db07e80cf52183` |
| [`pkg/statistics/region_totals.rs.md`](../../../pkg/statistics/region_totals.rs.md) | 60,61 | `21d7631924d9552ca320fa5c102340f425adbc748ca0f9401ad2e6ad89b29a17` |
| [`pkg/statistics/region_totals_test.rs`](../../../pkg/statistics/region_totals_test.rs) | 60,61 | `ae726f5b974d886e60c383bdbaa434df7c94c98694afd8ea50997d388f58f73d` |
| [`pkg/statistics/slow_stat.go`](../../../pkg/statistics/slow_stat.go) | 60,61 | `999b289b2cff2b5538d1194e390e29383cc34d0dcf70e5653c2d16bef31dcfac` |
| [`pkg/statistics/slow_stat.rs`](../../../pkg/statistics/slow_stat.rs) | 60,61 | `414cc99ad8f0e63cee201070c9efbf52b0e8c9cfd611314f6906338748f734c2` |
| [`pkg/statistics/slow_stat.rs.md`](../../../pkg/statistics/slow_stat.rs.md) | 60,61 | `71859479d42151cdface9e0b0f020b216dc10bc8e5c7b7ebfdbaaeac6d49c3fb` |
| [`pkg/statistics/slow_stat_test.rs`](../../../pkg/statistics/slow_stat_test.rs) | 60,61 | `da9d2d524243d5cf33fffc16e05678607a53aed1ebffca353148ef44001a82a2` |
| [`pkg/statistics/store.go`](../../../pkg/statistics/store.go) | 60,61 | `6375f9aaaabb2b69c703c0c3eb674c2d0aab7ac2d93c6713d1a46ab770ace2df` |
| [`pkg/statistics/store_collection.go`](../../../pkg/statistics/store_collection.go) | 60,61 | `a8c4bce992159a4c76de1373eb6cd6a2a48c524bb2cd5ab9c9b1ab76f4d05767` |
| [`pkg/statistics/store_collection.rs`](../../../pkg/statistics/store_collection.rs) | 60,61 | `ad56b23c56d232370ae790d38ea76acc6bcfa455fc19b1a67d0fd81d7a2893a4` |
| [`pkg/statistics/store_collection.rs.md`](../../../pkg/statistics/store_collection.rs.md) | 60,61 | `75962dcc6d0e193381fa093ef265ee71a0bdbf0b1ab3330409ba0d023bfc33ee` |
| [`pkg/statistics/store_collection_test.go`](../../../pkg/statistics/store_collection_test.go) | 60,61 | `53ec33cfe88de8a87fec011389be39d38e5669f2f15f0e54be1cc45375d58068` |
| [`pkg/statistics/store_collection_test.rs`](../../../pkg/statistics/store_collection_test.rs) | 60,61 | `fecad1b86d4089f5ae864bb3aa6b06bdab90d16c3d5bf692bb48471ab80cc5ce` |
| [`pkg/statistics/store_history.rs`](../../../pkg/statistics/store_history.rs) | 60,61 | `ea0eef1fbb96d607253cdee9f367b935cf44dc3fb346007b25dac039e78f1ae5` |
| [`pkg/statistics/store_history.rs.md`](../../../pkg/statistics/store_history.rs.md) | 60,61 | `88be625ecf7601447568e10fe543fd9077d365d3fa9ca3ee456b431cf6411837` |
| [`pkg/statistics/store_history_test.rs`](../../../pkg/statistics/store_history_test.rs) | 60,61 | `c669ca56f71de42e596611a2f52ae5e67abdee6cb168e78d9249627a09ce070d` |
| [`pkg/statistics/store_hot_peers_infos.go`](../../../pkg/statistics/store_hot_peers_infos.go) | 60,61 | `abac5bb0e3eacd2d1e12158e0e5131284ebacd63540f9ee21fe0f70566434c22` |
| [`pkg/statistics/store_load.go`](../../../pkg/statistics/store_load.go) | 60,61 | `3a1ad6b343965b66a39472b4621f7c0d0812c1f206decb9359438b5c34587c45` |
| [`pkg/statistics/store_load_test.go`](../../../pkg/statistics/store_load_test.go) | 60,61 | `3645c0b227414ebf6244d613cba7f5821a7874600071d35b402c4ef418f96c16` |
| [`pkg/statistics/store_rolling.rs`](../../../pkg/statistics/store_rolling.rs) | 60,61 | `bfd772146936f292ad2b591e232e3be3e51213420def9f4f238f7b9ca4949b6d` |
| [`pkg/statistics/store_rolling.rs.md`](../../../pkg/statistics/store_rolling.rs.md) | 60,61 | `c708081eb0a50bbbac92b20bc8e162d5c23c3d908355b09b6fbf1deca35dac55` |
| [`pkg/statistics/store_rolling_test.rs`](../../../pkg/statistics/store_rolling_test.rs) | 60,61 | `7b506c4bfc1faf90198fb931d782b8788f25b51dbabe4a908029849c50a9620e` |
| [`pkg/statistics/store_stat_informer.go`](../../../pkg/statistics/store_stat_informer.go) | 60,61 | `b14954aed85a945dbffe3158f2d2f40e6131f41560878c1718f4a360122f86f7` |
| [`pkg/statistics/store_test.go`](../../../pkg/statistics/store_test.go) | 60,61 | `4d231315c725e124ac9e50229c16ab17d97aaa4f02aa3b1b251c401e7521db65` |
| [`pkg/statistics/stores.rs`](../../../pkg/statistics/stores.rs) | 60,61 | `b607408b7fe2b1b284811e8adb6fb91eff0ad09a0b54dc98d3e29e6f2283c444` |
| [`pkg/statistics/stores.rs.md`](../../../pkg/statistics/stores.rs.md) | 60,61 | `c0f77480f7c8179a5d4287658e1eee987205477e1dc0b04bb3be977c1a5cabbe` |
| [`pkg/statistics/stores_test.rs`](../../../pkg/statistics/stores_test.rs) | 60,61 | `2a27d8f7d53304a92f32ed3dc0f349ab58338ef5acf7006f9c16048b8999ef6c` |
| [`pkg/statistics/utils/Cargo.toml`](../../../pkg/statistics/utils/Cargo.toml) | 60,61 | `378977025806d3a691c88cf43889ff5b6fe3b6dd63656d42736dcf1696dfe0f5` |
| [`pkg/statistics/utils/constant.go`](../../../pkg/statistics/utils/constant.go) | 60,61 | `8a69cc80a5204d93979ff91a3ba419b57a884602ca1f772b8611aaff7a1e9804` |
| [`pkg/statistics/utils/kind.go`](../../../pkg/statistics/utils/kind.go) | 60,61 | `94dc862cd7f3c091e461999a377e7097ce8ea75b53380a7983e29091c5711629` |
| [`pkg/statistics/utils/kind.rs`](../../../pkg/statistics/utils/kind.rs) | 60,61 | `9bef3a70159c4a7c858c4e75b6e9e9dabf1a405e6443dd343615b5411eb57cf4` |
| [`pkg/statistics/utils/kind.rs.md`](../../../pkg/statistics/utils/kind.rs.md) | 60,61 | `c227843d15010e27e474d71e1c787530c3d9d73b57abea31cec2a30074d6026d` |
| [`pkg/statistics/utils/kind_test.go`](../../../pkg/statistics/utils/kind_test.go) | 60,61 | `05c452da9ed3cfe624b426038bef8f5df93f8f53686fe0daa7fd0ffd27213bc4` |
| [`pkg/statistics/utils/kind_test.rs`](../../../pkg/statistics/utils/kind_test.rs) | 60,61 | `2938881a23169629776ae35607e572ba77c0ba9f165bc02390a1fa4a382cc846` |
| [`pkg/statistics/utils/labels.go`](../../../pkg/statistics/utils/labels.go) | 60,61 | `19166113aebb73729c10ab17845c933a4d61f4ab3bb8379065142430b885bde7` |
| [`pkg/statistics/utils/lib.rs`](../../../pkg/statistics/utils/lib.rs) | 60,61 | `f5672f578eee91a0c015dfc144ba0e2879f2d7ec1b98e05ee26a5ab0718c03e9` |
| [`pkg/statistics/utils/lib.rs.md`](../../../pkg/statistics/utils/lib.rs.md) | 60,61 | `742b97f84fed639c2294e7a01716ee9e5c3bbebf28c29d1a422584abe9fc5301` |
| [`pkg/statistics/utils/topn.go`](../../../pkg/statistics/utils/topn.go) | 60,61 | `c88697c0c9cd16fd2ebc47e34970e4e514ffed344ba3f4f7ec6b950e788aec79` |
| [`pkg/statistics/utils/topn.rs`](../../../pkg/statistics/utils/topn.rs) | 60,61 | `6922a07390dd106abedcd79bd9fa2d6755058904ed13c0b7bcb752d51bc4af36` |
| [`pkg/statistics/utils/topn.rs.md`](../../../pkg/statistics/utils/topn.rs.md) | 60,61 | `02d5c8c11a8ebc9cb63b50ff5df55b9fc9662685acaf915962a4ad60b6127710` |
| [`pkg/statistics/utils/topn_test.go`](../../../pkg/statistics/utils/topn_test.go) | 60,61 | `cc5c4402e4afbaa958e795e1030e2eadc2aacc501a78fe65b8c780e57445ccec` |
| [`pkg/statistics/utils/topn_test.rs`](../../../pkg/statistics/utils/topn_test.rs) | 60,61 | `b507b872c9dd0fb19fc64f8f9397f0590466e145cc428c264430aac8cdb014c1` |
| [`pkg/statistics/utils/topn_ttl.rs`](../../../pkg/statistics/utils/topn_ttl.rs) | 60,61 | `b3d5c2ddf497406fdd2c994711e24168af982fda6ebaaa51b0be39d860b480ec` |
| [`pkg/statistics/utils/topn_ttl.rs.md`](../../../pkg/statistics/utils/topn_ttl.rs.md) | 60,61 | `be4319bb72f909dffde7847d5b66fe33c1cddadf8e2464c0aee9590bbaa81074` |
| [`pkg/statistics/utils/topn_ttl_test.rs`](../../../pkg/statistics/utils/topn_ttl_test.rs) | 60,61 | `a06b946bd9ce8913f740781a5ad247cc25be846886ee378e3be722c9a86bebe0` |
| [`pkg/storage/Cargo.toml`](../../../pkg/storage/Cargo.toml) | 32,33 | `38a4d89ad4afd8d0f42734c310309425e9673f991bcb9a35f55d2c3fe1bc6319` |
| [`pkg/storage/domain/Cargo.toml`](../../../pkg/storage/domain/Cargo.toml) | 32,33 | `c48e45f2d142e4689d54a930c6b7c804e0cd42a400a2af40e43795a1d4aae11c` |
| [`pkg/storage/domain/endpoint.rs`](../../../pkg/storage/domain/endpoint.rs) | 32,33 | `1c2b687586e138dd1bdc8f8673103ce460cb619a93999bbf9948212eb324033e` |
| [`pkg/storage/domain/endpoint.rs.md`](../../../pkg/storage/domain/endpoint.rs.md) | 32,33 | `10653c57f4a1d1cff7312e4d9d525328545a82012e0a4b14a72c89e1645979ac` |
| [`pkg/storage/domain/kv.rs`](../../../pkg/storage/domain/kv.rs) | 32,33 | `9937d276ff5c06c83a055e072ca3b3e0d4da358f8bf5656e6a878244290c25c8` |
| [`pkg/storage/domain/kv.rs.md`](../../../pkg/storage/domain/kv.rs.md) | 32,33 | `cd940ef6f0785053e2985b39e6049c1292e9ff7bf9814c4bb8732f86ca5eac33` |
| [`pkg/storage/domain/lib.rs`](../../../pkg/storage/domain/lib.rs) | 32,33 | `dbc5ea3d6b89336797140c9be759c88822bc3038cf1241711995e3f53abb9871` |
| [`pkg/storage/domain/lib.rs.md`](../../../pkg/storage/domain/lib.rs.md) | 32,33 | `48e298f92ad53a5f44c8327e632ab0172fec8c0a80de13ff5124b0b7943e7034` |
| [`pkg/storage/endpoint/Cargo.toml`](../../../pkg/storage/endpoint/Cargo.toml) | 6,32,33 | `78dba15d324bb90198356513d1cded29a2e21e1b609a0e665277636e74c1e4e7` |
| [`pkg/storage/endpoint/affinity.go`](../../../pkg/storage/endpoint/affinity.go) | 6,32,33 | `b2f8c3f6fe1c718fae3d2fdc2d28f6f62e4c82009f11ca3de68abaacb32ec42c` |
| [`pkg/storage/endpoint/affinity_endpoint.rs`](../../../pkg/storage/endpoint/affinity_endpoint.rs) | 6,32,33 | `bc56b8875b5ae73a7133f06070bc9fb13bf974b356cd08264322cd68b51ccbb0` |
| [`pkg/storage/endpoint/affinity_endpoint_test.rs`](../../../pkg/storage/endpoint/affinity_endpoint_test.rs) | 6,32,33 | `34a3c45201b267990f25b0d8824d32b889d836053372a522aef74d42502be8ac` |
| [`pkg/storage/endpoint/batch_ops.rs`](../../../pkg/storage/endpoint/batch_ops.rs) | 6,32,33 | `5c5196fddbc5dc9f780909dcec727abf0d5ad54ea235c253e686f8f6ab84c511` |
| [`pkg/storage/endpoint/batch_ops_test.rs`](../../../pkg/storage/endpoint/batch_ops_test.rs) | 6,32,33 | `55044469378c12b9eff1da10f8b8df03026513dd5c3218918a089d257eed37f0` |
| [`pkg/storage/endpoint/cluster_id.go`](../../../pkg/storage/endpoint/cluster_id.go) | 6,32,33 | `e999033081b896ccdc8db10aed1374fa37a2485527d49b292fed777d5e2ceaf2` |
| [`pkg/storage/endpoint/cluster_id.rs`](../../../pkg/storage/endpoint/cluster_id.rs) | 6,32,33 | `47ba8797b844b5d1028d5472f82c5eb93a2b3b5a9106ccedb531d2c7b14977a8` |
| [`pkg/storage/endpoint/cluster_id_test.go`](../../../pkg/storage/endpoint/cluster_id_test.go) | 6,32,33 | `a59824725631223e887797d82f186e92f2a0c4077621b3ee4ab9458ee6e1870c` |
| [`pkg/storage/endpoint/cluster_id_test.rs`](../../../pkg/storage/endpoint/cluster_id_test.rs) | 6,32,33 | `d861b2a681adb2eace8630e4c24844ebffcc4e8a44a0bc941a1674eda8660701` |
| [`pkg/storage/endpoint/cluster_meta.rs`](../../../pkg/storage/endpoint/cluster_meta.rs) | 6,32,33 | `fa3b2841cb5763ef15c8e5e130f2fc6ecfbb362579e73d1ed7155f54884c2af8` |
| [`pkg/storage/endpoint/cluster_meta_test.rs`](../../../pkg/storage/endpoint/cluster_meta_test.rs) | 6,32,33 | `18f38b3770dfa4b3315bc4113da1ed1fb4050ae03f84d23818ec33115cea14ce` |
| [`pkg/storage/endpoint/config.go`](../../../pkg/storage/endpoint/config.go) | 6,32,33 | `80b88b85f84a52a612193c4c1f0b031db28d053f6ef35f8ac69896be4a5ecdec` |
| [`pkg/storage/endpoint/config_endpoint.rs`](../../../pkg/storage/endpoint/config_endpoint.rs) | 6,32,33 | `ab52eb0a497e301213ff65bd4b1ff5fdda1f81c743352245ccf12f9965a7e8ca` |
| [`pkg/storage/endpoint/config_endpoint_test.rs`](../../../pkg/storage/endpoint/config_endpoint_test.rs) | 6,32,33 | `e8795a2796f23ca0ada48504f02502091fdbd2f85ffe6aab1bd062f73e502e7b` |
| [`pkg/storage/endpoint/endpoint.go`](../../../pkg/storage/endpoint/endpoint.go) | 6,32,33 | `a80a322e2edd66d55d3407eafac44e78ac02f989c8709f408ca8d44d6f61b74a` |
| [`pkg/storage/endpoint/endpoint_codec.rs`](../../../pkg/storage/endpoint/endpoint_codec.rs) | 6,32,33 | `6ff3b3930b1a9311fc3bef9857a176e56ada2af10076a060fe7d756058a1ca77` |
| [`pkg/storage/endpoint/endpoint_codec.rs.md`](../../../pkg/storage/endpoint/endpoint_codec.rs.md) | 6,32,33 | `c688df3214c7cffa0f72a3a4d9cd7fb3567992061e3f0d9893b01f3f1b5b81e2` |
| [`pkg/storage/endpoint/endpoint_codec_test.rs`](../../../pkg/storage/endpoint/endpoint_codec_test.rs) | 6,32,33 | `40c94dd745237580d867cebc00592e846ca03d558185785d8ae41e3f1a19616e` |
| [`pkg/storage/endpoint/external_timestamp.go`](../../../pkg/storage/endpoint/external_timestamp.go) | 6,32,33 | `af197109719a3fac3d6d11670c60a86dab655605694c256db9a3e028b25ef391` |
| [`pkg/storage/endpoint/gc_barrier_read.rs`](../../../pkg/storage/endpoint/gc_barrier_read.rs) | 6,32,33 | `eb021a1ee497e3b7606cbe4a651a4cd96d60785b0263a6b1d0edb74882cb1039` |
| [`pkg/storage/endpoint/gc_barrier_read_test.rs`](../../../pkg/storage/endpoint/gc_barrier_read_test.rs) | 6,32,33 | `98ca5823332b18f87b710f232f23438fd922017f24d30e0f27e3d4a483511e21` |
| [`pkg/storage/endpoint/gc_barrier_txn.rs`](../../../pkg/storage/endpoint/gc_barrier_txn.rs) | 6,32,33 | `37fe914dc2258eb4dcfe1786c513a42e4fedb493d42dd2de61ef7c67d093368b` |
| [`pkg/storage/endpoint/gc_barrier_txn_test.rs`](../../../pkg/storage/endpoint/gc_barrier_txn_test.rs) | 6,32,33 | `4eaa620acb3ad02821ef2529d925217e33d1c7b084d6afd1a5dd81644e4d059c` |
| [`pkg/storage/endpoint/gc_barrier_value.rs`](../../../pkg/storage/endpoint/gc_barrier_value.rs) | 6,32,33 | `cc251997ec059531ec8aece7563ebc1e7491adecdae3ff55c70649e510791f4f` |
| [`pkg/storage/endpoint/gc_barrier_value_test.rs`](../../../pkg/storage/endpoint/gc_barrier_value_test.rs) | 6,32,33 | `2090990d7ce54908670656a8c7e645a36773c67e3a505b7c23eb06054bf94326` |
| [`pkg/storage/endpoint/gc_batch_read.rs`](../../../pkg/storage/endpoint/gc_batch_read.rs) | 6,32,33 | `d3939e29ed813ab00cc4bdd0e5f1f2de465bf44d1fe30ac5af04f32840e38a57` |
| [`pkg/storage/endpoint/gc_batch_read_test.rs`](../../../pkg/storage/endpoint/gc_batch_read_test.rs) | 6,32,33 | `7600032c7d3abeb78cf534b41bbdfd176293713ce09b1b9678e251d8aafac934` |
| [`pkg/storage/endpoint/gc_safepoint_read.rs`](../../../pkg/storage/endpoint/gc_safepoint_read.rs) | 6,32,33 | `5c3c173bf5344d94452363d08f8378f2755ae7efb94c6fd746011101a4726146` |
| [`pkg/storage/endpoint/gc_safepoint_read_test.rs`](../../../pkg/storage/endpoint/gc_safepoint_read_test.rs) | 6,32,33 | `cf008cc30f7c182ea2090b4eea0f98fd526782580985dde387e0d84a6dc88a86` |
| [`pkg/storage/endpoint/gc_state_batch.go`](../../../pkg/storage/endpoint/gc_state_batch.go) | 6,32,33 | `ed68de6055e24b936fadcbd9e9db2f1cc049a41ac27443924b15a4fc6374974f` |
| [`pkg/storage/endpoint/gc_state_batch_test.go`](../../../pkg/storage/endpoint/gc_state_batch_test.go) | 6,32,33 | `552146d148164580c0842f62daf189e94a0323dd9f9a3b7e23eb60dd990aaf0a` |
| [`pkg/storage/endpoint/gc_state_txn.rs`](../../../pkg/storage/endpoint/gc_state_txn.rs) | 6,32,33 | `891a080b916ee96456621b96b8452601a6f2e33e175a0fb81da97b8f133f09e9` |
| [`pkg/storage/endpoint/gc_state_txn_test.rs`](../../../pkg/storage/endpoint/gc_state_txn_test.rs) | 6,32,33 | `25b5a026ea1ea5bec5c35ee103b1de84d015de7b32bc903d59626da4caedfda4` |
| [`pkg/storage/endpoint/gc_states.go`](../../../pkg/storage/endpoint/gc_states.go) | 6,32,33 | `5bab038897a0c9145817f396dfcbc525881aa4692522575657e2d48adc82ebca` |
| [`pkg/storage/endpoint/gc_states_test.go`](../../../pkg/storage/endpoint/gc_states_test.go) | 6,32,33 | `d7ce3e67b5955dfa03be69e4b97466fb876abd19dd7ffc527a33e2b3052042dd` |
| [`pkg/storage/endpoint/keyspace.go`](../../../pkg/storage/endpoint/keyspace.go) | 6,32,33 | `1fc82600b7ca2ff87b5e6b9efd280e82634556a76993716286ef88a5c25e1dec` |
| [`pkg/storage/endpoint/keyspace_group_state.rs`](../../../pkg/storage/endpoint/keyspace_group_state.rs) | 6,32,33 | `673f853d3119c9196dbdc5fcff6898fea6c9ded829fa936fd88b0d3da0325483` |
| [`pkg/storage/endpoint/keyspace_group_state_test.rs`](../../../pkg/storage/endpoint/keyspace_group_state_test.rs) | 6,32,33 | `14d828dff3c8432a9b8ff3f46f556987487aeafdf52808d655ed811696dfcc82` |
| [`pkg/storage/endpoint/keyspace_group_store.rs`](../../../pkg/storage/endpoint/keyspace_group_store.rs) | 6,32,33 | `7374dc467b6cfcfcbc7c42b47226625112c3fe9925f4b07f09f73d5fedadd81d` |
| [`pkg/storage/endpoint/keyspace_group_store_test.rs`](../../../pkg/storage/endpoint/keyspace_group_store_test.rs) | 6,32,33 | `692726a3d5df9137d8ab5e4e964fcbce169bab570b7c920fab72225b2999b5ee` |
| [`pkg/storage/endpoint/keyspace_group_value.rs`](../../../pkg/storage/endpoint/keyspace_group_value.rs) | 6,32,33 | `8584ee668ebcdba761aeaaf25e94be0a2a8e917eacc35936aafb60a8b2fa3d38` |
| [`pkg/storage/endpoint/keyspace_group_value_test.rs`](../../../pkg/storage/endpoint/keyspace_group_value_test.rs) | 6,32,33 | `1bfbb56d21bb4dbe228f40f4d9677d3d5c9fbb36024753d56aba20b2999fb47c` |
| [`pkg/storage/endpoint/keyspace_meta.rs`](../../../pkg/storage/endpoint/keyspace_meta.rs) | 6,32,33 | `7ecfc54e27aad7d7dc91aba3640d8ad47dc94c18192de1603d9ac04faaac2477` |
| [`pkg/storage/endpoint/keyspace_meta_test.rs`](../../../pkg/storage/endpoint/keyspace_meta_test.rs) | 6,32,33 | `be479051df1c0b05c3831d09f737ccea49cba1136cc9a8fd536c92326be44679` |
| [`pkg/storage/endpoint/keyspace_scan_test.rs`](../../../pkg/storage/endpoint/keyspace_scan_test.rs) | 6,32,33 | `4c7dcb15bb23d3a1bb622946f55c508c3fa580a1336c8f803dcc6b080d86697d` |
| [`pkg/storage/endpoint/lib.rs`](../../../pkg/storage/endpoint/lib.rs) | 6,32,33 | `02b1482312060c03d264bc92465ac498df5fa915ee572b898d1971aca3bbe23e` |
| [`pkg/storage/endpoint/maintenance.go`](../../../pkg/storage/endpoint/maintenance.go) | 6,32,33 | `0ca713699c1acda65d95f6f1b05d8eb1524a359a79c9daea21705b05048622d2` |
| [`pkg/storage/endpoint/maintenance_delete.rs`](../../../pkg/storage/endpoint/maintenance_delete.rs) | 6,32,33 | `0c1b4419160a82fff665c9b327d899f28a1963136f2ddfc6e1949b8fa8659efb` |
| [`pkg/storage/endpoint/maintenance_delete_test.rs`](../../../pkg/storage/endpoint/maintenance_delete_test.rs) | 6,32,33 | `4ae08c312e246b797fb74238c9a310013246ae7b694fc252cfc114ae070b1bf2` |
| [`pkg/storage/endpoint/maintenance_start.rs`](../../../pkg/storage/endpoint/maintenance_start.rs) | 6,32,33 | `21060ea9c50a3b78bf8c9972e25824ac000fd30b153f0ca795ec345fcd5e488d` |
| [`pkg/storage/endpoint/maintenance_start_test.rs`](../../../pkg/storage/endpoint/maintenance_start_test.rs) | 6,32,33 | `72a4e4b61e7338a8384cc5d7462c305deabc4db6bf7924c063ced6cdc3a940c2` |
| [`pkg/storage/endpoint/meta.go`](../../../pkg/storage/endpoint/meta.go) | 6,32,33 | `408e661468c0db03676d895e888abb66519caaf65dd03a3e4adb46c1fef02f86` |
| [`pkg/storage/endpoint/meta_service_group.go`](../../../pkg/storage/endpoint/meta_service_group.go) | 6,32,33 | `be3ab6af2aefb6696a2fd18f788616f3b9b96bc447bc3503ed075dba6397503f` |
| [`pkg/storage/endpoint/meta_service_group.rs`](../../../pkg/storage/endpoint/meta_service_group.rs) | 6,32,33 | `0554550d900c13f66c4b575ad2c480c524192fc80ad90b9e4a682236d2ad20e6` |
| [`pkg/storage/endpoint/meta_service_group_test.rs`](../../../pkg/storage/endpoint/meta_service_group_test.rs) | 6,32,33 | `8eac8525f367eaffcf26802b0827849a50d700cbe92a119feecffef1cc4e08e4` |
| [`pkg/storage/endpoint/min_resolved_ts.go`](../../../pkg/storage/endpoint/min_resolved_ts.go) | 6,32,33 | `54bcf77a022955bc7d50e729f436b3a898a1a79989634a1ca83db94a18680a35` |
| [`pkg/storage/endpoint/region_meta.rs`](../../../pkg/storage/endpoint/region_meta.rs) | 6,32,33 | `666d9b8458567408cf7ec387755646ad38135aeb78aed99c3c2ab3f684da20ad` |
| [`pkg/storage/endpoint/region_meta_test.rs`](../../../pkg/storage/endpoint/region_meta_test.rs) | 6,32,33 | `722fcbf2f1efd226b87714fa6773baac7bf2aa06121af8cc07cbcef2533d2def` |
| [`pkg/storage/endpoint/region_scan.rs`](../../../pkg/storage/endpoint/region_scan.rs) | 6,32,33 | `88f76e54ecd597d6c6c0faef64b7e219046a0e8e1eb529774a81beb25f00e0cd` |
| [`pkg/storage/endpoint/region_scan_test.rs`](../../../pkg/storage/endpoint/region_scan_test.rs) | 6,32,33 | `65c03db331087173cfe371938f0dadba6277728e268a3c07e4c4d3cb4351530f` |
| [`pkg/storage/endpoint/replication_endpoint.rs`](../../../pkg/storage/endpoint/replication_endpoint.rs) | 6,32,33 | `ca145d02ef5768c2224debfc6fb486f64baa514a3f0c4708c1a8dbee681985cb` |
| [`pkg/storage/endpoint/replication_endpoint_test.rs`](../../../pkg/storage/endpoint/replication_endpoint_test.rs) | 6,32,33 | `fff57f7db95e9c979df9d47d01dc6bec2c2da90683a9e7454d9fd2df8516ae02` |
| [`pkg/storage/endpoint/replication_status.go`](../../../pkg/storage/endpoint/replication_status.go) | 6,32,33 | `fd0d7dbb0bf07fab2eff6bb52f6e2b8dd76e3edcfe0b6b85fa8656c44cc04a08` |
| [`pkg/storage/endpoint/resource_group.go`](../../../pkg/storage/endpoint/resource_group.go) | 6,32,33 | `04f7141302601a2cb056df4260abe78fcaf125360b976d40a35ecf42a9edf1e1` |
| [`pkg/storage/endpoint/resource_settings.rs`](../../../pkg/storage/endpoint/resource_settings.rs) | 6,32,33 | `1798feae5f366cbb3396beb516425dbb96f98386b47fa8c2a804482ca9bd9633` |
| [`pkg/storage/endpoint/resource_settings_test.rs`](../../../pkg/storage/endpoint/resource_settings_test.rs) | 6,32,33 | `79542eb8b7f7338846f8c31faadfa90f9f5a52017ef7019d79ee860052e5faed` |
| [`pkg/storage/endpoint/resource_states.rs`](../../../pkg/storage/endpoint/resource_states.rs) | 6,32,33 | `1994f0e9ac7b6ce6290acd1cc89b466de49564aab4a84ede6cd239850cbfa1e0` |
| [`pkg/storage/endpoint/resource_states_test.rs`](../../../pkg/storage/endpoint/resource_states_test.rs) | 6,32,33 | `8053b6d08c3d03349e14329a3c2b38057374fbb7816176afcf6e2219f817a964` |
| [`pkg/storage/endpoint/rule.go`](../../../pkg/storage/endpoint/rule.go) | 6,32,33 | `e944b020f8ab27e9d578aa751ad4118de2c96ce523a6006f33c78e7295d4ec13` |
| [`pkg/storage/endpoint/rule_endpoint.rs`](../../../pkg/storage/endpoint/rule_endpoint.rs) | 6,32,33 | `a91c84f2e352b2f7f3db673211a5d83c19b430456b5831b03efcc2409b80d9ed` |
| [`pkg/storage/endpoint/rule_endpoint_test.rs`](../../../pkg/storage/endpoint/rule_endpoint_test.rs) | 6,32,33 | `6296ec9ac01f346165531c0a6ad4886f753a2b8f63a2c803c34c7b3ac14a0fa9` |
| [`pkg/storage/endpoint/scheduler_endpoint.rs`](../../../pkg/storage/endpoint/scheduler_endpoint.rs) | 6,32,33 | `c0055a3c09147067720327ea6369fbf737f4b6601381272131d07798386d29b6` |
| [`pkg/storage/endpoint/scheduler_endpoint_test.rs`](../../../pkg/storage/endpoint/scheduler_endpoint_test.rs) | 6,32,33 | `ddc53a123dc0ac32249d33b711431bb6c712c212071d1f4f9a14c6e2c6b3fdaa` |
| [`pkg/storage/endpoint/service_limits.rs`](../../../pkg/storage/endpoint/service_limits.rs) | 6,32,33 | `937c85da4ea6ff1bfca6de1eb9b72d826fbedead3bcd95288a09610a3a6596c7` |
| [`pkg/storage/endpoint/service_limits_test.rs`](../../../pkg/storage/endpoint/service_limits_test.rs) | 6,32,33 | `7db8adeee6f2955f00ae568af55510b56d5b37477535e9bdd402abd9b8907010` |
| [`pkg/storage/endpoint/service_middleware.go`](../../../pkg/storage/endpoint/service_middleware.go) | 6,32,33 | `95d01d5e28e27210574ef90ebba7b1a4b5200647bda5f11d759337f56003a549` |
| [`pkg/storage/endpoint/store_meta.rs`](../../../pkg/storage/endpoint/store_meta.rs) | 6,32,33 | `2d1c96019b21ad801a86b298b71fbaf4921e1610302da8365b25e2880780db00` |
| [`pkg/storage/endpoint/store_meta_test.rs`](../../../pkg/storage/endpoint/store_meta_test.rs) | 6,32,33 | `0d8e7337bbaa10b46ee5a7e2794a409a565a1c589598513dafbe066664e4eb80` |
| [`pkg/storage/endpoint/store_weights.rs`](../../../pkg/storage/endpoint/store_weights.rs) | 6,32,33 | `feffcb3f6756fb542cf4f790995bd5cb370079c687ff45478684f6e3338b7461` |
| [`pkg/storage/endpoint/store_weights_test.rs`](../../../pkg/storage/endpoint/store_weights_test.rs) | 6,32,33 | `354b107cd269b374f4954b839508349aadf1bafb9f36e45211a6849b4a1243ce` |
| [`pkg/storage/endpoint/timestamp_fenced_save.rs`](../../../pkg/storage/endpoint/timestamp_fenced_save.rs) | 6,32,33 | `0299c7fb67fce89e42fac8006d05606ff8204f0d9c1b72333bf8f11f9b7a1eb0` |
| [`pkg/storage/endpoint/timestamp_fenced_save_test.rs`](../../../pkg/storage/endpoint/timestamp_fenced_save_test.rs) | 6,32,33 | `1ee27d6cecc8f4cf6babdcf0c74ce96d85dc2a1e16ecdadc780891a3b7ae81aa` |
| [`pkg/storage/endpoint/timestamp_values.rs`](../../../pkg/storage/endpoint/timestamp_values.rs) | 6,32,33 | `f8192b5cbed5443c194290d8624c34f96b3518cc5481947dede84cf4ffcf791d` |
| [`pkg/storage/endpoint/timestamp_values_test.rs`](../../../pkg/storage/endpoint/timestamp_values_test.rs) | 6,32,33 | `3febb92aea6771f8df145149b15390f4054148749d228d54404df5e6a7c15952` |
| [`pkg/storage/endpoint/timestamp_window.rs`](../../../pkg/storage/endpoint/timestamp_window.rs) | 6,32,33 | `16ab219695fdc25852f5f9d8e2532e2113e431982a56dd2f39b7b7174c014eae` |
| [`pkg/storage/endpoint/timestamp_window_test.rs`](../../../pkg/storage/endpoint/timestamp_window_test.rs) | 6,32,33 | `c0dd19bae58216ee34a1ab5531a1c587eee2f9862e7009dca7d7f2f343416426` |
| [`pkg/storage/endpoint/tso.go`](../../../pkg/storage/endpoint/tso.go) | 6,32,33 | `3301344fb56e0eaa7612db39123c739b69a52ffd1a2a08cdbea16c2c6fdf237a` |
| [`pkg/storage/endpoint/tso_keyspace_group.go`](../../../pkg/storage/endpoint/tso_keyspace_group.go) | 6,32,33 | `d628861f2265cb37d9f358f26a5b9bde2f54ba48e66f8f53087659f6f75a22d1` |
| [`pkg/storage/endpoint/tso_keyspace_group_test.go`](../../../pkg/storage/endpoint/tso_keyspace_group_test.go) | 6,32,33 | `bab2917acad0784c16e3c907aae00ee2b98d01dc76f44904c67fd94feab26c40` |
| [`pkg/storage/endpoint/util.go`](../../../pkg/storage/endpoint/util.go) | 6,32,33 | `fd42c0c95d8b826eba0df43446f564d369eb0e55209445fe54ea78f8f66b9eea` |
| [`pkg/storage/etcd_backend.go`](../../../pkg/storage/etcd_backend.go) | 32,33 | `b0727c3072c0f909248809b4229d09a05812befdf9d367492d218952a569b3ea` |
| [`pkg/storage/facade.rs`](../../../pkg/storage/facade.rs) | 32,33 | `0c77b54f2651b14ef055b20e5f9e6035ce9badb6b72bad14f58439c9ba45da19` |
| [`pkg/storage/facade_test.rs`](../../../pkg/storage/facade_test.rs) | 32,33 | `35d3554a6c3f2a12a90726614231c2f605817c90fad6145315e379ea4a8c32d4` |
| [`pkg/storage/hot_region_storage.go`](../../../pkg/storage/hot_region_storage.go) | 32,33 | `bb147edaaaecd80057ece8f1e84842f628fe0ee606fbc19f0a1d031b861e6cde` |
| [`pkg/storage/hot_region_storage_test.go`](../../../pkg/storage/hot_region_storage_test.go) | 32,33 | `c5e7852ab876b418a741c22cfbc7234b15da1bb51beb8c7d4485a03c37afc0c2` |
| [`pkg/storage/keyspace_test.go`](../../../pkg/storage/keyspace_test.go) | 32,33 | `e85c60b64a8c7bcca953013a9d9fe3b274806d901875b3bf589b76f4aaed40d7` |
| [`pkg/storage/kv/Cargo.toml`](../../../pkg/storage/kv/Cargo.toml) | 32,33 | `c0d7b0e3259c6d0729881ee140becb2dba7e0f0e9cab69f0b82a528a9d3a7394` |
| [`pkg/storage/kv/base_kv.rs`](../../../pkg/storage/kv/base_kv.rs) | 32,33 | `b07d3f198b7423f53d8f1a1815213d438062b27a7f74a87b9cf268f0eef6778c` |
| [`pkg/storage/kv/base_kv_test.rs`](../../../pkg/storage/kv/base_kv_test.rs) | 32,33 | `aab77253d9c7a9e93119f84ebf4bddfedd032c4a9376b1746926975aa851c9d1` |
| [`pkg/storage/kv/bounded_range_test.rs`](../../../pkg/storage/kv/bounded_range_test.rs) | 32,33 | `1e91b37ba8f0247f2f694b66b1d35a9c0a639c669409f173515d67976e647822` |
| [`pkg/storage/kv/callback_txn.rs`](../../../pkg/storage/kv/callback_txn.rs) | 32,33 | `103daa9f8c3a8147ee1c472462ca7c6d8be30c3592a6803b7fc3b73d830c36ce` |
| [`pkg/storage/kv/callback_txn_test.rs`](../../../pkg/storage/kv/callback_txn_test.rs) | 32,33 | `74dbd0d19b953f9023d5f01118712a697e00045553bd8b9e6612eab276e0d53a` |
| [`pkg/storage/kv/contract_test.rs`](../../../pkg/storage/kv/contract_test.rs) | 32,33 | `5651eea06911a8e06e83b14ca727e015ecd1e3215351d233c2f6a7740db01a84` |
| [`pkg/storage/kv/etcd_fixture_test.rs`](../../../pkg/storage/kv/etcd_fixture_test.rs) | 4,32,33 | `9737e08a8320f45982d1a9dfa0ac968aa3174425490d4f491c218371d4454609` |
| [`pkg/storage/kv/etcd_kv.go`](../../../pkg/storage/kv/etcd_kv.go) | 32,33 | `164444b80b892ca9ed7a8d9b174de7644c72ad54de59043d0f2d9affa41382a8` |
| [`pkg/storage/kv/etcd_store.rs`](../../../pkg/storage/kv/etcd_store.rs) | 32,33 | `63f7f266b588f23dcc75d0f54906f84bbcb4620225531de6b0045974746cf235` |
| [`pkg/storage/kv/etcd_store_test.rs`](../../../pkg/storage/kv/etcd_store_test.rs) | 32,33 | `28f6487fd6c54d32aa8e4f5cde18728ff62a66fd073cb5107ca3b3009d36712f` |
| [`pkg/storage/kv/kv.go`](../../../pkg/storage/kv/kv.go) | 32,33 | `963c1139aa64bb0ed3f81483c043c75626addc1ca86416a8fef3ff3a7ea6117c` |
| [`pkg/storage/kv/kv_test.go`](../../../pkg/storage/kv/kv_test.go) | 32,33 | `1cb2e4e6bcc7c29240cd286e22a78c8cfbd29d6a10c0af73dc8e8ff5d9a0179e` |
| [`pkg/storage/kv/levedb_kv.go`](../../../pkg/storage/kv/levedb_kv.go) | 32,33 | `e6e45a2eaeb7ee0512c165a81ef2ecf70635faf92a7dcac6ce225a57523d7040` |
| [`pkg/storage/kv/lib.rs`](../../../pkg/storage/kv/lib.rs) | 32,33 | `87afb332f1d66133eaab14ce9ac3539054c4fb033ac427952e0a70553dde5864` |
| [`pkg/storage/kv/lib.rs.md`](../../../pkg/storage/kv/lib.rs.md) | 32,33 | `6295a787c20920b6bd60f1d7aeebe06a9750ce284f23b39fabc4eb3fe0d3d67c` |
| [`pkg/storage/kv/mem_kv.go`](../../../pkg/storage/kv/mem_kv.go) | 32,33 | `2776eb0f1526a71b7ed7fa881f71a7cd24ec877e5d6e53ea78b9e00a40edda76` |
| [`pkg/storage/kv/memory_store.rs`](../../../pkg/storage/kv/memory_store.rs) | 32,33 | `b35893423df4f210a428e9680f8d6ca5c935081e4be28d854802ce49fcd7392b` |
| [`pkg/storage/kv/meta_store.rs`](../../../pkg/storage/kv/meta_store.rs) | 32,33 | `478d1c7b1a61f4532213386d8c36a88f08c0ada559f439fae3e265feee36f24d` |
| [`pkg/storage/kv/meta_store_test.rs`](../../../pkg/storage/kv/meta_store_test.rs) | 32,33 | `5adb186c4d2afcd2cc2bb49152e52bc18d29a262f02f2066f65f303bc6ba081b` |
| [`pkg/storage/kv/metrics.go`](../../../pkg/storage/kv/metrics.go) | 32,33 | `eb5c263366aca6ab102aebd42a492c522a2ffcd2eb26ebaf324759e3cb9c3136` |
| [`pkg/storage/kv/raw_branches.rs`](../../../pkg/storage/kv/raw_branches.rs) | 32,33 | `b24e291c533807375bf971e7854f5e28c24c56645879dc839f7a989b5dcd3423` |
| [`pkg/storage/kv/raw_branches_test.rs`](../../../pkg/storage/kv/raw_branches_test.rs) | 32,33 | `8a9eb27fe62e1d24f49c624a23d6f2679a4024f3ae85aa08ef377f6b6eef3c3f` |
| [`pkg/storage/kv/raw_compare.rs`](../../../pkg/storage/kv/raw_compare.rs) | 32,33 | `fc5498a91bef2a7e58a9c1fbdc51bdfd3ae09b8660dd61fde807a680a5ac2d1d` |
| [`pkg/storage/kv/raw_compare_test.rs`](../../../pkg/storage/kv/raw_compare_test.rs) | 32,33 | `1b6de28a4bd2d1c2236d0066f4bac09a7ed350f56ff9b6be111073052f2abafa` |
| [`pkg/storage/kv/raw_get.rs`](../../../pkg/storage/kv/raw_get.rs) | 32,33 | `5341c50340b489611d4a806db55b7ecb9356327a685d9341e26cf5d66dc40da7` |
| [`pkg/storage/kv/raw_get_test.rs`](../../../pkg/storage/kv/raw_get_test.rs) | 32,33 | `31b46d7c01f9802e266b09497bdf111ece904c0fa93c622196ce24d4e3bc0553` |
| [`pkg/storage/kv/raw_range.rs`](../../../pkg/storage/kv/raw_range.rs) | 32,33 | `54de780acd92dcd3a35733a7d292b2934a6a270c30a27adc1da31c712034101f` |
| [`pkg/storage/kv/raw_range_test.rs`](../../../pkg/storage/kv/raw_range_test.rs) | 32,33 | `5b7fa13c60e15ff620a87ae5022dd6b3b5be34b95365cd71906cc098997846b1` |
| [`pkg/storage/kv/revision_txn.rs`](../../../pkg/storage/kv/revision_txn.rs) | 32,33 | `01981d1369904b86f3007b3416bd9bcf24ad0535c0c641adedd35da2a6c54f4a` |
| [`pkg/storage/kv/revision_txn_test.rs`](../../../pkg/storage/kv/revision_txn_test.rs) | 32,33 | `ea8608f8d0f2739471dd51049d91835b419696d6232bf83e3132507e75c01faf` |
| [`pkg/storage/kv/txn_cancel.rs`](../../../pkg/storage/kv/txn_cancel.rs) | 32,33 | `b5052ebc425a84403fca272f73637a04f9006c630b17979301796fb0976ae429` |
| [`pkg/storage/kv/txn_cancel_test.rs`](../../../pkg/storage/kv/txn_cancel_test.rs) | 32,33 | `05171a946a43f3b0bc8ab08c9e8e1535844fc09cb885bde4695600dab3c78bec` |
| [`pkg/storage/leveldb_backend.go`](../../../pkg/storage/leveldb_backend.go) | 32,33 | `db2c3e01e3a568c6374eb744d9ad3f875934e2233d2dd06b59102c869c52df26` |
| [`pkg/storage/leveldb_backend_test.go`](../../../pkg/storage/leveldb_backend_test.go) | 32,33 | `9532a73723ebba3e4c138dcd7a427559b8d1044f1e084465f6a1859f53500ed0` |
| [`pkg/storage/lib.rs`](../../../pkg/storage/lib.rs) | 32,33 | `9e97e6e5c6249d47daec0dd5098f0c7a1fd8023ee2b5eb70ccf6948ac9c132f7` |
| [`pkg/storage/memory_backend.go`](../../../pkg/storage/memory_backend.go) | 32,33 | `0150ffb393188c647fba7ecdf39badfb5a60d18743f0a25b196247c0b8e23f96` |
| [`pkg/storage/meta_service_group_test.go`](../../../pkg/storage/meta_service_group_test.go) | 32,33 | `8d10efbb234572a57a9d15dfb9aa0076ed254db7d3fea07ab10fb24e298afb47` |
| [`pkg/storage/raft-prototype/Cargo.toml`](../../../pkg/storage/raft-prototype/Cargo.toml) | 32,33 | `afc5bd87127957da8bebf83dbf220f5e5d07e46e8510566708344992beb06b05` |
| [`pkg/storage/raft-prototype/cluster_test.rs`](../../../pkg/storage/raft-prototype/cluster_test.rs) | 32,33 | `67a57d1400b9c1112005bf779745963b769326550ad129441f423765e8f8a409` |
| [`pkg/storage/raft-prototype/lib.rs`](../../../pkg/storage/raft-prototype/lib.rs) | 32,33 | `7511ff24199542e1efb4f036b51fd3fd001a28ca44b7d2eb2a302cf7c7e815ff` |
| [`pkg/storage/raft-prototype/log_store.rs`](../../../pkg/storage/raft-prototype/log_store.rs) | 32,33 | `bd47f074bbe4f67be296314aa3e8082ef84b6e2adc36f76c57547252b1b8e885` |
| [`pkg/storage/raft-prototype/node.rs`](../../../pkg/storage/raft-prototype/node.rs) | 32,33 | `4555cc60689c53de8b051d5ddeb41332f1354f8dd28357adc4caae11e5114660` |
| [`pkg/storage/raft-prototype/state_machine.rs`](../../../pkg/storage/raft-prototype/state_machine.rs) | 32,33 | `a93b28e38ec91e1dacb9ee23baec2f4325fd61b141e8365c3e47ce01a371537c` |
| [`pkg/storage/region_lifecycle.rs`](../../../pkg/storage/region_lifecycle.rs) | 32,33 | `66db0922f882e511c520f87fde6609b59c9630ddb3d6b25d1cc2b98deba47ce6` |
| [`pkg/storage/region_lifecycle_test.rs`](../../../pkg/storage/region_lifecycle_test.rs) | 32,33 | `2a062e0063c72327070bc6fd40214ebf2bb655c8bb750f7d6a17965428742e80` |
| [`pkg/storage/region_load_once.rs`](../../../pkg/storage/region_load_once.rs) | 32,33 | `1f4fd5ae9226a477de2b31bb13f0c3a24ced8e31d2b8f6c7f9629b964001b2ed` |
| [`pkg/storage/region_load_once_test.rs`](../../../pkg/storage/region_load_once_test.rs) | 32,33 | `4f109cb74bd493129931f6f8cacf83babdf0aec0be7fe93ade87daa4f9c12884` |
| [`pkg/storage/region_storage.go`](../../../pkg/storage/region_storage.go) | 32,33 | `8ebbd4541b1f87f770e2af983830f88c6b1ca6d573a15509f91e7d3cdadfdd17` |
| [`pkg/storage/region_storage_test.go`](../../../pkg/storage/region_storage_test.go) | 32,33 | `b7aeb666e7373574137febe67abef9aebc6bbe9011ba4b0c177fb6aeb5f95aa3` |
| [`pkg/storage/region_switch.rs`](../../../pkg/storage/region_switch.rs) | 32,33 | `5ebc3fd49401a11e6edef2a854e6f2c60dcf949cd75ecd20fff3a66c4df219d9` |
| [`pkg/storage/region_switch_test.rs`](../../../pkg/storage/region_switch_test.rs) | 32,33 | `00058fa6038ee14bcc4c95cb78f8f6f69ec4bff171d3f0cb02401de581bbd6f0` |
| [`pkg/storage/storage.go`](../../../pkg/storage/storage.go) | 32,33 | `35f90a410efe7db0b6f31c5a8cac78d7e3f7fd58d1187b9d7472b3acd00b3530` |
| [`pkg/storage/storage_resource_group_test.go`](../../../pkg/storage/storage_resource_group_test.go) | 32,33 | `599e112f581971f9aca99a7a639e206ddad59f7983d0d6f23eae51a43f02968e` |
| [`pkg/storage/storage_test.go`](../../../pkg/storage/storage_test.go) | 32,33 | `4b4bd765715c87faf6e5cd4798b071dd3cf20d608a93f02b2d2d639c5ac672db` |
| [`pkg/storage/storage_tso_test.go`](../../../pkg/storage/storage_tso_test.go) | 32,33 | `ecffb4ade09d62de679914261f664d5c8c38b48dc5f4facffc7f4f764687d8ad` |
| [`pkg/syncer/Cargo.toml`](../../../pkg/syncer/Cargo.toml) | 11,95 | `7cc740010856a0068ce5cf9f028bee385a4c022c4c45be10e84f99866c3e61dd` |
| [`pkg/syncer/client.go`](../../../pkg/syncer/client.go) | 11,95 | `1670929d4d4766dc69ae40c132eb109035f8f671b95b6e37ac261349c8ea44ec` |
| [`pkg/syncer/client_apply.rs`](../../../pkg/syncer/client_apply.rs) | 11,95 | `246779975d83feee065f04ed2cd39a2ee61f1ae0c52d1d7859dedbc94a2d94a7` |
| [`pkg/syncer/client_apply.rs.md`](../../../pkg/syncer/client_apply.rs.md) | 11,95 | `c09295f15917b10f9e618a16a92f91afb2452ea301ca5b1f00369ee142b63612` |
| [`pkg/syncer/client_apply_test.rs`](../../../pkg/syncer/client_apply_test.rs) | 11,95 | `9cc6942a1985c2aeb192b11f4cce02a881ce5efea18545615dccc45f099543be` |
| [`pkg/syncer/client_ready.rs`](../../../pkg/syncer/client_ready.rs) | 11,95 | `3c80f772c69922b27c21529a53a5646f5b5aebfb396b5db612ff0dddd04e8f0a` |
| [`pkg/syncer/client_ready.rs.md`](../../../pkg/syncer/client_ready.rs.md) | 11,95 | `4ef6e2ffe3c07f4a52ff37b3b076781c474f0293daa5b07d0a08c3916001691f` |
| [`pkg/syncer/client_ready_test.rs`](../../../pkg/syncer/client_ready_test.rs) | 11,95 | `7404405be9ba8511fbc15a27059fa09a6758b17e23db0d44f6fa955bab3175ce` |
| [`pkg/syncer/client_test.go`](../../../pkg/syncer/client_test.go) | 11,95 | `0ac415fa0559ae4e36e4b93058bb910829c3314d415b3047e2727fd2d5abbae0` |
| [`pkg/syncer/downstream.rs`](../../../pkg/syncer/downstream.rs) | 11,95 | `df3ae9908c02f94aecbfe02ad6e573db75f72329f8d3dcd92705d398524ff779` |
| [`pkg/syncer/downstream.rs.md`](../../../pkg/syncer/downstream.rs.md) | 11,95 | `902fb06c6026daa88250630b814da79ce9cfb6f1a4c14e128e076a3c268d6a7f` |
| [`pkg/syncer/downstream_test.rs`](../../../pkg/syncer/downstream_test.rs) | 11,95 | `04c9174bcbf72b677ce4997733d287fabafa5679f9a0e9ccf812519f8b43db87` |
| [`pkg/syncer/full_catchup.rs`](../../../pkg/syncer/full_catchup.rs) | 11,95 | `df6f1b3a5e930d022413624070ae64692db48f61d6b78c2b1bf89437150d6e36` |
| [`pkg/syncer/full_catchup.rs.md`](../../../pkg/syncer/full_catchup.rs.md) | 11,95 | `a8a518b7d8707e98109b0a1a11eea14e31ab06e029abed86857ed7e9612e06f5` |
| [`pkg/syncer/full_catchup_test.rs`](../../../pkg/syncer/full_catchup_test.rs) | 11,95 | `c8470afc115eacad0b81b1165a923742f11cfa7d921168dfd11fcf7b340fe764` |
| [`pkg/syncer/full_snapshot.rs`](../../../pkg/syncer/full_snapshot.rs) | 11,95 | `90fd112d92f1f2b882ea4e9945b46cd6f099bf98c992789e082b989ca5ad6627` |
| [`pkg/syncer/full_snapshot.rs.md`](../../../pkg/syncer/full_snapshot.rs.md) | 11,95 | `aac490d77cd18970968136aa2ae178940ab2f788780045ef43465e7c9b07dfed` |
| [`pkg/syncer/full_snapshot_test.rs`](../../../pkg/syncer/full_snapshot_test.rs) | 11,95 | `c7790bdd1ea588114c938e9828259adcd458e72c973934c08b31f0ff4fc05ae0` |
| [`pkg/syncer/history_buffer.go`](../../../pkg/syncer/history_buffer.go) | 11,95 | `8b37dba3613e46db4c1a3f4d8b2d545c28862f45af60a48e5f58ee56dc6ba1b0` |
| [`pkg/syncer/history_buffer.rs`](../../../pkg/syncer/history_buffer.rs) | 11,95 | `a0c3d3fb22bd8f836f75fca765c6a39ec0109c0a515e3a80e38883531d26b253` |
| [`pkg/syncer/history_buffer.rs.md`](../../../pkg/syncer/history_buffer.rs.md) | 11,95 | `4f10020c1eca406f678a6091ce6d9ca514d863cef19e104088782b5e9419e9a5` |
| [`pkg/syncer/history_buffer_test.go`](../../../pkg/syncer/history_buffer_test.go) | 11,95 | `9264f9d6df7e3abc630681cef9e63af0dcecce91ae3c5fdd6c936af7bd92fdaa` |
| [`pkg/syncer/history_buffer_test.rs`](../../../pkg/syncer/history_buffer_test.rs) | 11,95 | `5a6eb2cb23f0e2dc58cee234535ceef4e77375c9629fa755db4c433932a4917e` |
| [`pkg/syncer/history_persist.rs`](../../../pkg/syncer/history_persist.rs) | 11,95 | `f0985df8396bfcd45eaf2e2506efc8d7c167ad9ebd13eb871123396d0c9a124b` |
| [`pkg/syncer/history_persist.rs.md`](../../../pkg/syncer/history_persist.rs.md) | 11,95 | `1759fda9b5c8cc100681b66402300bf43a2f5d4ae8cc37f1d611bb6097464e1d` |
| [`pkg/syncer/history_persist_test.rs`](../../../pkg/syncer/history_persist_test.rs) | 11,95 | `efb75278ccb1212a6fcdb9c5bb06240cb64611288011e38464453663e700f1e0` |
| [`pkg/syncer/history_replay.rs`](../../../pkg/syncer/history_replay.rs) | 11,95 | `a84a53e587d9560c29779143da77895b86d78dcc4795ee93505fa7fe7cbd9798` |
| [`pkg/syncer/history_replay.rs.md`](../../../pkg/syncer/history_replay.rs.md) | 11,95 | `643afce2e6bcfdf2c206ee435bec4f9212b9eab226aba09bbe8b4e90a462db7b` |
| [`pkg/syncer/history_replay_test.rs`](../../../pkg/syncer/history_replay_test.rs) | 11,95 | `666ad244b8276ebd584001a0ce377d9a5944fd717470da79f2bacfbd8fa9834d` |
| [`pkg/syncer/history_window.rs`](../../../pkg/syncer/history_window.rs) | 11,95 | `9846dc1b593fbc7db8165bb2c962d7ce680204f29566e54cacb225c3026a0aa9` |
| [`pkg/syncer/history_window.rs.md`](../../../pkg/syncer/history_window.rs.md) | 11,95 | `d7751c09a5f3f9b3243dbbcca3614c482a07b45aede4ed63ad9120b186f7efcb` |
| [`pkg/syncer/history_window_test.rs`](../../../pkg/syncer/history_window_test.rs) | 11,95 | `3d030dc029b3aa26c76168740776e552fe89f6c5a29661ce55b91bca2498ae91` |
| [`pkg/syncer/lib.rs`](../../../pkg/syncer/lib.rs) | 11,95 | `57ed98ceaa80ef7625fd455d3126716e1429780bee045ada3634bc246a3266fe` |
| [`pkg/syncer/lib.rs.md`](../../../pkg/syncer/lib.rs.md) | 11,95 | `f9d121f6e62fa720010c0a2ea73dcb5f86cca1a3c76d5981591575823b2c21d7` |
| [`pkg/syncer/lifecycle.rs`](../../../pkg/syncer/lifecycle.rs) | 11,95 | `88e6d2b6a7e66f3a2b2193317f3dc9a44652e072e3499ab1723768b2a847e1b4` |
| [`pkg/syncer/lifecycle.rs.md`](../../../pkg/syncer/lifecycle.rs.md) | 11,95 | `c04c5dde708977a3bc13892f1a8ebf89077b0c528baa2e23bac906fda285fcfe` |
| [`pkg/syncer/lifecycle_test.rs`](../../../pkg/syncer/lifecycle_test.rs) | 11,95 | `52cb1e1310660436513e2717623475670e485f58b4b2a6a73be55d3693b99cf3` |
| [`pkg/syncer/metadata_watch.rs`](../../../pkg/syncer/metadata_watch.rs) | 11,95 | `f997d70bfb627a49666b5981a35bb67cd1a346378360c83d3386df112fc864f6` |
| [`pkg/syncer/metadata_watch.rs.md`](../../../pkg/syncer/metadata_watch.rs.md) | 11,95 | `c820d75d8b64fd3c9136acc39e8006b1e057b7e3f564abede46095fff863fcf4` |
| [`pkg/syncer/metadata_watch_test.rs`](../../../pkg/syncer/metadata_watch_test.rs) | 11,95 | `a7746bdf0485a31e7b1eeefefcf560cb02e31fbed9d4925dd1c6754c6e203a61` |
| [`pkg/syncer/metrics.go`](../../../pkg/syncer/metrics.go) | 11,95 | `fd836c2fe7d8c48f266bf3f1de08ea1f232460160392592c07874bab0895f235` |
| [`pkg/syncer/server.go`](../../../pkg/syncer/server.go) | 11,95 | `488e6aed50f74f9499c6a5c9a9fa1dd32693de91bd72781ddbadbd8c09068200` |
| [`pkg/syncer/server_test.go`](../../../pkg/syncer/server_test.go) | 11,95 | `6a71a5929f34387742a4ccf34908c7928aa5ed557231a1a62a1b2d575ed206ed` |
| [`pkg/tso/Cargo.toml`](../../../pkg/tso/Cargo.toml) | 35,36,75,76 | `7ed7e8d6bff4259c64efbfee706856cd22efc295ba841c023655688221ba89b3` |
| [`pkg/tso/admin.go`](../../../pkg/tso/admin.go) | 35,36,75,76 | `fd91f8204afced8f7a129b7c2173933da02a2ce98223a8399e39824f75c532ab` |
| [`pkg/tso/allocator.go`](../../../pkg/tso/allocator.go) | 35,36,75,76 | `c6759b6e68397424525420bf950f838182351a87c69f682e7fac509af27f3690` |
| [`pkg/tso/allocator_test.go`](../../../pkg/tso/allocator_test.go) | 35,36,75,76 | `a7a66a06e0506a563e03223169056a861186cc32135977f71d1e39f77c4fb1e5` |
| [`pkg/tso/compat_test.json`](../../../pkg/tso/compat_test.json) | 35,36,75,76 | `44ee19e0d8e994acb11d6f19b8222d168e3db89652a1c01a3101fa518e18ad9b` |
| [`pkg/tso/config.go`](../../../pkg/tso/config.go) | 35,36,75,76 | `0455fc4770f940f43ca85766325a4ee4b3c091065e601b3c17636d10497613ad` |
| [`pkg/tso/etcd_behavior_test.rs`](../../../pkg/tso/etcd_behavior_test.rs) | 35,36,75,76 | `1f69ce3bc395bbee64140541a9220ad6a4d74ad2788f4f5eb14738e374da5b8e` |
| [`pkg/tso/keyspace_group_manager.go`](../../../pkg/tso/keyspace_group_manager.go) | 35,36,75,76 | `5d1a0ecee15c07b4e58568a1a23edc91f08b0919df379783c0406234844e546d` |
| [`pkg/tso/keyspace_group_manager_test.go`](../../../pkg/tso/keyspace_group_manager_test.go) | 35,36,75,76 | `9f42f932be35a4f74c197d1205b820137fadda91f1f2f1ba3a39ec14a66b39cc` |
| [`pkg/tso/lib.rs`](../../../pkg/tso/lib.rs) | 35,36,75,76 | `299448468904be177f727184931e72975dad2da31b6fa20de186fd13fb37bed4` |
| [`pkg/tso/metrics.go`](../../../pkg/tso/metrics.go) | 35,36,75,76 | `3d72684d82ea06e3e324cefe597482fa8af6b31a1654fd188946c88483dee425` |
| [`pkg/tso/oracle.rs`](../../../pkg/tso/oracle.rs) | 35,36,75,76 | `f3ad764c089ace9ef8878d8819d153c50d72789ab100a0f87b2e20956f495f5c` |
| [`pkg/tso/oracle_test.rs`](../../../pkg/tso/oracle_test.rs) | 35,36,75,76 | `fde3031558097660faf033a9088cc98da409d2a6425973e6fc22a536ef6b1426` |
| [`pkg/tso/restart_test.rs`](../../../pkg/tso/restart_test.rs) | 35,36,75,76 | `b9489ab9664b750045e9f0468b2fcc5489b81fde411f6232a70f1e92546ad3f7` |
| [`pkg/tso/rust_oracle_test.go`](../../../pkg/tso/rust_oracle_test.go) | 35,36,75,76 | `922312ca9176621482181ff98670fb87ec0effd156599761beb2abcfb642d0df` |
| [`pkg/tso/testutil.go`](../../../pkg/tso/testutil.go) | 35,36,75,76 | `fa9fa3f237e11c5a712a88f15d02e012ef41c35a3557e26e880d74234defeb8e` |
| [`pkg/tso/tso.go`](../../../pkg/tso/tso.go) | 35,36,75,76 | `330d66e48fad1a89f96388da68596afa6b9bcadcc49046ff61a51c2be5b5b789` |
| [`pkg/tso/tso_test.go`](../../../pkg/tso/tso_test.go) | 35,36,75,76 | `e2ee84275ab3156d523c4a74ee94c049f49406c2503ea5b5639cacbfd2780456` |
| [`pkg/tso/util.go`](../../../pkg/tso/util.go) | 35,36,75,76 | `1a3e228a557e7c14feb84a89eabc67c7cd1b3dcbcda4f46a3356eb9ae42cb3ae` |
| [`pkg/tso/util_test.go`](../../../pkg/tso/util_test.go) | 35,36,75,76 | `42645e6751ac27a825ed9bc34bf1b28c902d1f58651192112f8f2aa8a8ceda58` |
| [`pkg/utils/apiutil/apiutil.go`](../../../pkg/utils/apiutil/apiutil.go) | 38,41,82 | `edda400120b9f6cc3def157e8aefc74e4572b7971d01b521f41b6bee49e6bfe6` |
| [`pkg/utils/apiutil/apiutil_test.go`](../../../pkg/utils/apiutil/apiutil_test.go) | 41,82 | `f7e8636ea8527283afeea992a3da3acdc5de6830cad54902a6f069be8d7cf1cd` |
| [`pkg/utils/apiutil/multiservicesapi/middleware.go`](../../../pkg/utils/apiutil/multiservicesapi/middleware.go) | 41,82 | `487a734a337a24d7aa9c362a7f291955df503f2253e39b1b75447840fee140a7` |
| [`pkg/utils/apiutil/serverapi/middleware.go`](../../../pkg/utils/apiutil/serverapi/middleware.go) | 40,41,82 | `1ade7b1de2ce0bd5d46768bf100a2ef371215cc4aead1e463b9436cd42a92a0c` |
| [`pkg/utils/apiutil/serverapi/middleware_test.go`](../../../pkg/utils/apiutil/serverapi/middleware_test.go) | 41,82 | `94749a7549dafa18783a234d7b6a8be42aec030c1654bbe440cd1eb303bb88d8` |
| [`pkg/utils/grpcutil/Cargo.toml`](../../../pkg/utils/grpcutil/Cargo.toml) | 42,82,83,84,85 | `3d2d617db34ba14713fe43dd261e0f2fa93cf4531e55d35f275ebda6d53cde29` |
| [`pkg/utils/grpcutil/cluster.go`](../../../pkg/utils/grpcutil/cluster.go) | 42,82,83,84,85 | `7a265f1b78c2551517f1c8aa7ab085eeb0338c2f2d6425d11baf3283495dc6cb` |
| [`pkg/utils/grpcutil/cluster_test.go`](../../../pkg/utils/grpcutil/cluster_test.go) | 42,82,83,84,85 | `2b878a5e8e0e5b7853ef725bf19585ec8edd524fdb4d12d4f7351889696b1762` |
| [`pkg/utils/grpcutil/config_test.rs`](../../../pkg/utils/grpcutil/config_test.rs) | 42,82,83,84,85 | `c0e906b73bedd1574e77df7e082e77557d5d20a028402e71a67da76c19dffd23` |
| [`pkg/utils/grpcutil/grpcutil.go`](../../../pkg/utils/grpcutil/grpcutil.go) | 42,82,83,84,85 | `e8523a4888c1fd2a0ee2d8cc6e1b914b25736eee359afa4d6f49e998b422613b` |
| [`pkg/utils/grpcutil/grpcutil_test.go`](../../../pkg/utils/grpcutil/grpcutil_test.go) | 42,82,83,84,85 | `ce715fe91f301c4a102cad1f287f3a5a9f18fccf8024691d554177e60b19e014` |
| [`pkg/utils/grpcutil/lib.rs`](../../../pkg/utils/grpcutil/lib.rs) | 42,82,83,84,85 | `c1617455420244d0aec29f8fb07626c89f76ff806e4a253e0b5b85b8f640070b` |
| [`pkg/utils/grpcutil/metrics.go`](../../../pkg/utils/grpcutil/metrics.go) | 42,82,83,84,85 | `a52919d1f9dbaa5dcc506f3b37dc78b4519d8099f888a35dfd71f8cecc7bf2a4` |
| [`pkg/utils/keypath/Cargo.toml`](../../../pkg/utils/keypath/Cargo.toml) | 6,20,32,33,34,35,48,49,50,69,70,71 | `7821abc8e33f94ad667f6f125f3488e952ef5d0f19f237b69512265964bdc522` |
| [`pkg/utils/keypath/absolute_key_path.go`](../../../pkg/utils/keypath/absolute_key_path.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `84b851ba31ba4f974622c5e4b2331387eb11108b3299fe88a5df67ace26a88d3` |
| [`pkg/utils/keypath/affinity.go`](../../../pkg/utils/keypath/affinity.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `49973f569f0171e43680bea2a3b7d458b45ab8feb59714eaa2d857667cbaff3b` |
| [`pkg/utils/keypath/cluster_id.go`](../../../pkg/utils/keypath/cluster_id.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `e63a257d38836ed66b584cf5ed984f7efc1585a7fb95bb039bfc07d246eac292` |
| [`pkg/utils/keypath/cluster_paths.rs`](../../../pkg/utils/keypath/cluster_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `8375b83369e53f51b569e9996704445f10ec24980557604a3d4db17c10cafd75` |
| [`pkg/utils/keypath/cluster_paths_test.rs`](../../../pkg/utils/keypath/cluster_paths_test.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `fdfb7cf1d960c1e9d56c6192e93d40a5e9f6269286f0819b069299f20f9d7ece` |
| [`pkg/utils/keypath/config.go`](../../../pkg/utils/keypath/config.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `6c18b9aabfd6d01b3bb487c93dc7afcf697626394f38344489f6484d85366c09` |
| [`pkg/utils/keypath/election.go`](../../../pkg/utils/keypath/election.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `7735930f48835c307d57f501b58e240b5a375d2cad02cdcb4bb6b54b6c9a47ca` |
| [`pkg/utils/keypath/gc_paths.rs`](../../../pkg/utils/keypath/gc_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `91bcbfcb89ddf74d246bcc9ef8f46054f68ed7678e74fb8bae86d23b7bce4d05` |
| [`pkg/utils/keypath/gc_paths_test.rs`](../../../pkg/utils/keypath/gc_paths_test.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `ab758e77f6c8273b032b69d5493c4d10568a856dbf975a2979484167c7384ab7` |
| [`pkg/utils/keypath/gc_states.go`](../../../pkg/utils/keypath/gc_states.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `bd9ceba1a4c512c0b2e5ac87de254c4be07b00d505c6fcff25fedfd2a52bd1fa` |
| [`pkg/utils/keypath/keyspace_time_paths.rs`](../../../pkg/utils/keypath/keyspace_time_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `d1eb80856777b0ec6c59212609ceb6497f2107fc3db8fa1a3e9bbcf3cd2f246c` |
| [`pkg/utils/keypath/keyspace_time_paths_test.rs`](../../../pkg/utils/keypath/keyspace_time_paths_test.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `ff716c7a8a4655d6777b16c90acb857ea72efcbea6c631e1ff1bfc2bec8f8406` |
| [`pkg/utils/keypath/lib.rs`](../../../pkg/utils/keypath/lib.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `a2b0bbf435cff16ab0b0c53a424ff005801e32bb606997c151cf97c045b66a2e` |
| [`pkg/utils/keypath/maintenance.go`](../../../pkg/utils/keypath/maintenance.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `961f8bd61f113be9d27b6ecde00561b0d8a755e6222c6ed828d6e877323b86ae` |
| [`pkg/utils/keypath/member.go`](../../../pkg/utils/keypath/member.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `1569b2eac91fbe8aa57166f06bff45767b684ae78411826d8e14a66b4c439754` |
| [`pkg/utils/keypath/member_config_paths.rs`](../../../pkg/utils/keypath/member_config_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `d86c6403155367910888c40b57e8b52a31ebd114841d5db55323373f1bb9cec2` |
| [`pkg/utils/keypath/member_config_paths_test.rs`](../../../pkg/utils/keypath/member_config_paths_test.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `87efdb99da28d0d3c3ee74485062875c1d9b5d3e8b6af379da9871c994f7fb62` |
| [`pkg/utils/keypath/region_test.go`](../../../pkg/utils/keypath/region_test.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `40c5942039bbece703b947e4bf4bba2a8350e8e7e3aa0e44cd6ada9d48bae225` |
| [`pkg/utils/keypath/registry_marker_paths.rs`](../../../pkg/utils/keypath/registry_marker_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `182100a8fddf56c531ccd80c6f4b68df34fb7206b5e36a7726064be0b9eb434d` |
| [`pkg/utils/keypath/registry_marker_paths_test.rs`](../../../pkg/utils/keypath/registry_marker_paths_test.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `5bb13349aec9f7dfacf0e94dbc91c135ea91fb851473314476726888ed11ce37` |
| [`pkg/utils/keypath/resource_group.go`](../../../pkg/utils/keypath/resource_group.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `6bf06facf89fed595f2d3a8d229af5cbda2f6f59303a86f14ff3e01c77ee920a` |
| [`pkg/utils/keypath/resource_group_test.go`](../../../pkg/utils/keypath/resource_group_test.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `d5689cd481f5051f1925bbff2129561c4c9ecd59cfff9e72d6d86de3808e5be5` |
| [`pkg/utils/keypath/rule.go`](../../../pkg/utils/keypath/rule.go) | 6,20,32,33,34,35,48,49,50,69,70,71 | `c2da8c59eed2223abe6ee5039b00d4443a9301094611d956f3fb2896ccb4bbab` |
| [`pkg/utils/keypath/rule_resource_paths.rs`](../../../pkg/utils/keypath/rule_resource_paths.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `3610279f1eaca5d29797237e4ef34111c9064fcd6ab4ae6f5234c1c68ad90ccc` |
| [`pkg/utils/keypath/rule_resource_paths_test.rs`](../../../pkg/utils/keypath/rule_resource_paths_test.rs) | 6,20,32,33,34,35,48,49,50,69,70,71 | `2bcaa6bcb0120a7844f859653f176228529b14ed45bbad1b435f0f76386034d2` |
| [`pkg/utils/logutil/Cargo.toml`](../../../pkg/utils/logutil/Cargo.toml) | 92 | `6a686f63613a40f078aba7878f7520b6b37f258fed4e1db1be1b555d0bf8e5d1` |
| [`pkg/utils/logutil/config_test.rs`](../../../pkg/utils/logutil/config_test.rs) | 92 | `999e77a65d6b77d01188f26b748fe3925946459baafae276260905dba4af6440` |
| [`pkg/utils/logutil/lib.rs`](../../../pkg/utils/logutil/lib.rs) | 92 | `0ad4794d3f2c5ba90861a5325fb8718e76bc77a85a1e4e10b4b1bb9c2bee20fb` |
| [`pkg/utils/logutil/log.go`](../../../pkg/utils/logutil/log.go) | 92 | `1e46ee3cd5268e6ec3bb94d1f66172603db78fea255f08f281ed5e67582a7ba5` |
| [`pkg/utils/logutil/log_test.go`](../../../pkg/utils/logutil/log_test.go) | 92 | `09ec6047a4e07c4d43609f5a1619d70cab81fc569795eb7f27e13e12a380fa8a` |
| [`pkg/utils/memberutil/member.go`](../../../pkg/utils/memberutil/member.go) | 88 | `faaff08dc2103c76e6ea6737a16ac9cca308bb333068c7f86fafe732602455b7` |
| [`pkg/utils/metricutil/Cargo.toml`](../../../pkg/utils/metricutil/Cargo.toml) | 86 | `6bb1b284c8dbbc267ac7933f159664874915945bf78e93ae0a2cc22b963e88a2` |
| [`pkg/utils/metricutil/config_test.rs`](../../../pkg/utils/metricutil/config_test.rs) | 86 | `2011e411b1dc4a236d95ae8262f151cbb2f7e5c1e029166f1f67954e42639df8` |
| [`pkg/utils/metricutil/lib.rs`](../../../pkg/utils/metricutil/lib.rs) | 86 | `d87d1560da5b18917f00aa224cd8462847505db304ee7df5ea2afbde9185a0c2` |
| [`pkg/utils/metricutil/metricutil.go`](../../../pkg/utils/metricutil/metricutil.go) | 86 | `8000f3e27167f7b36f6c6b33a38bc241264ffb7e57ac49f8267715cd56fd2e8e` |
| [`pkg/utils/metricutil/metricutil_test.go`](../../../pkg/utils/metricutil/metricutil_test.go) | 86 | `0acd50a31056ebd4f67dda4debf5ce1308c3bc743a1fa8d528161820974e4f49` |
| [`pkg/utils/netutil/address.go`](../../../pkg/utils/netutil/address.go) | 39 | `5edf6ab6c8f6c54fdd30cc6e5e10fdcd36400eaeb25acef11ef7c0e405bdc1c8` |
| [`pkg/utils/netutil/address_test.go`](../../../pkg/utils/netutil/address_test.go) | 102 | `0e5818f8316dc33a980536f61196e332ab831308ec4af218aff97e4fd876c075` |
| [`pkg/utils/requestutil/context.go`](../../../pkg/utils/requestutil/context.go) | 40,81 | `9807ccf05b5cb966d926421c2b03d8c292d4349cdb8fe5311bc9d664d75c8834` |
| [`pkg/utils/requestutil/context_test.go`](../../../pkg/utils/requestutil/context_test.go) | 40,81 | `793e90168f8fd6ab93489760fc1136eb09db7c640109bd9db20e0364eb97d031` |
| [`pkg/utils/requestutil/request_info.go`](../../../pkg/utils/requestutil/request_info.go) | 40,81 | `4f42252a9b69958fc66b36f503fe5bc0a0b13012b6d0ef4d2f1af6e38cf0a403` |
| [`pkg/utils/requestutil/request_info_test.go`](../../../pkg/utils/requestutil/request_info_test.go) | 40,81 | `6bc6fcfb214986bf33ffc90c0d88fe3dd563c6df4470794fac889f3a068cc730` |
| [`pkg/utils/tempurl/check_env_dummy.go`](../../../pkg/utils/tempurl/check_env_dummy.go) | 39 | `35c74257116866211e8fb81dd9a07d7a57425bff866e94ba406e10594f628646` |
| [`pkg/utils/tempurl/check_env_linux.go`](../../../pkg/utils/tempurl/check_env_linux.go) | 39 | `220a2e28c02ea8155df0872d89cb6ecd68d031c4dc550c2c6b861df5a9d1d365` |
| [`pkg/utils/tempurl/tempurl.go`](../../../pkg/utils/tempurl/tempurl.go) | 39 | `9610e60dac2ab801c6f6203909124e131b93a39eb66bff80b5b7343d023de71f` |
| [`pkg/utils/tempurl/tempurl_test.go`](../../../pkg/utils/tempurl/tempurl_test.go) | 39 | `76c3373dc21e64a8814ae94699f494bbd4de445bc81d0d823fc0a6f1e26f68ed` |
| [`scripts/run-pd-compat.sh`](../../../scripts/run-pd-compat.sh) | 4 | `9494e94a2711f4980f4c7ce8866b3ddf63f557f4333b22e73a7e28b18ff0800b` |
| [`server/api/admin.go`](../../../server/api/admin.go) | 73,74,75,76 | `7f744abdcc6ca50b1d558c72800e3b522d89b21ee727ddca40e2d053708aba83` |
| [`server/api/checker.go`](../../../server/api/checker.go) | 66 | `635d8b50d0aef329cac208fe603e68a724fe60b2d06de5497b33d9aceb969864` |
| [`server/api/cluster.go`](../../../server/api/cluster.go) | 46 | `3a043e5d6f8e6f6e391f2e7bb56fa202f436603670bf1c480cac5d374765d7b7` |
| [`server/api/config.go`](../../../server/api/config.go) | 47,48,49,50 | `2420b655f05de478cf7d82146c55622b362e4762d93847341300ae2065f1cdf8` |
| [`server/api/config_test.go`](../../../server/api/config_test.go) | 102 | `0e5b3854cd07c37f81da889b12caf242e9f663c6d9735980edff49827a8b2725` |
| [`server/api/diagnostic.go`](../../../server/api/diagnostic.go) | 66 | `cc03e3c125e6c7fadd11765b553240a00900d1735895ddcf47862299302dee90` |
| [`server/api/failpoint.go`](../../../server/api/failpoint.go) | 102 | `d9bee6355cf677b4c05de785fff69643c9c84e4b8d5f6f3824639f0905915cd5` |
| [`server/api/health.go`](../../../server/api/health.go) | 43 | `07003b99d61d038bc3ac20a701cde2bc91b3c12e60911521ef76fd5e6c312a1e` |
| [`server/api/health_test.go`](../../../server/api/health_test.go) | 102 | `a19e63f85f7d1346a3de23804314542cda4c651f42297cca3b203c2dd8fd36b5` |
| [`server/api/hot_status.go`](../../../server/api/hot_status.go) | 60,61 | `90ea7431edbbdc8a4f38b50981c69d3ced79bb3e8a68c624121a7f227ae9a7e2` |
| [`server/api/label.go`](../../../server/api/label.go) | 51,53 | `cf9c07c181695bd3260de5cc80a14b961c80d82da395b20b1760101cb9cd52bc` |
| [`server/api/log.go`](../../../server/api/log.go) | 92 | `9ab4596182c76de12ffd5f85d5f5e437d9c63cb5441b13621861f7333e8ae9fe` |
| [`server/api/member.go`](../../../server/api/member.go) | 44,45 | `1d0e529defe327ae323ac08192bc9b1e8b886dcf844bb77a62a5db1d9dfb6a8b` |
| [`server/api/metric.go`](../../../server/api/metric.go) | 102 | `ca22793a7a204ea3f7e4f67a91a61ee322d9a05d6f8450f0473a36ee82cef41e` |
| [`server/api/metric_test.go`](../../../server/api/metric_test.go) | 102 | `796a550bc8cebaf33a6aa192efec13606830c820379cc087ba5a46622efeb908` |
| [`server/api/middleware.go`](../../../server/api/middleware.go) | 38,41,79,80,81 | `b33e56c1f1540e3ce02a1d6552f7707b446b0f169b271fa004cdded1865dca92` |
| [`server/api/middleware_test.go`](../../../server/api/middleware_test.go) | 102 | `ae96a8f6cb9d07f6c265060e84b9e7a6faa0551e8d67e3bf8ff6f11079a368e4` |
| [`server/api/min_resolved_ts.go`](../../../server/api/min_resolved_ts.go) | 77 | `c46af4053d42868b8e368fbfed7e836f4e075eac997200df7b17c09dc1efc8eb` |
| [`server/api/operator.go`](../../../server/api/operator.go) | 62,63 | `0d9f709be1417d25cf226d1175cdd39bcdddd41ce8ab14c5dd1af92cb38639aa` |
| [`server/api/plugin.go`](../../../server/api/plugin.go) | 102 | `81fcb99751fe637b1406dfec56a8e20a58692853832e3ea9e37cfb9bfc704a3b` |
| [`server/api/plugin_disable.go`](../../../server/api/plugin_disable.go) | 102 | `f3511a1963840a629a89e113aff362db480124a66b1c98485734e7af2074cd02` |
| [`server/api/pprof.go`](../../../server/api/pprof.go) | 102 | `dc7a0d560e4104c00fd72bc58dfeaa106ccc9d806a68f0551ff76bc82600f165` |
| [`server/api/region.go`](../../../server/api/region.go) | 55,56,57,58,59,67 | `128ce9a38d73fca90399cfa5a1cf6b08fc4a2cd47a63f9fe5e84c4e50810b8cc` |
| [`server/api/region_label.go`](../../../server/api/region_label.go) | 70 | `75e1885f044be26ba3ddb6ae2d259d16ec9c81b4eb475626bc333030a36797d3` |
| [`server/api/region_test.go`](../../../server/api/region_test.go) | 102 | `a72a8101f046818ad0c146da9dea84ae6a07528fd6f5a19a09ba25e6dd9cd784` |
| [`server/api/replication_mode.go`](../../../server/api/replication_mode.go) | 102 | `e707b73cb765761d96e43b9d43ceb788276e3e1e4c1ef0ac5ae2d8906deac437` |
| [`server/api/router.go`](../../../server/api/router.go) | 2,81,101 | `153c7396117af4561c33e9dfe0529e1a0ef45ba5304fa9e41600cd7e60eb2b79` |
| [`server/api/rule.go`](../../../server/api/rule.go) | 68,69 | `08ad927023ab3ec8ba1b4cdb8f47b00f9886b2eabd35096c651a335a5889b721` |
| [`server/api/scheduler.go`](../../../server/api/scheduler.go) | 64,65 | `e5ab3f98213b91406b52945499713532c44079514cda7693bdd2bca6471044a3` |
| [`server/api/server.go`](../../../server/api/server.go) | 102 | `7af50a6a093565d22a089f1f26bdf4666306107b288627cdde3f6aa4d1877664` |
| [`server/api/service_gc_safepoint.go`](../../../server/api/service_gc_safepoint.go) | 102 | `6ce6168fa79ab0b80030ddde5362f8811d00333a63b4cc77c04fcdd6e583c70e` |
| [`server/api/service_middleware.go`](../../../server/api/service_middleware.go) | 3,79,80 | `149297e7a7b961d4d1b02b1992850051f4549b57da83df94da321a36a95dd86b` |
| [`server/api/stats.go`](../../../server/api/stats.go) | 57,58 | `9fad3d28545377510b7026d3c9f581b1ce2c5573903f7c922d1f69625b4e80a6` |
| [`server/api/status.go`](../../../server/api/status.go) | 46 | `c1f63902109c096ea5f68147a720587834f29db829b089a64dfa927d2e0d665a` |
| [`server/api/store.go`](../../../server/api/store.go) | 51,52,53,54 | `22ca2275949654672b9b168758e6e95c268273c7410627a742a00352b016ba4e` |
| [`server/api/store_test.go`](../../../server/api/store_test.go) | 102 | `aedac0df6445b0a86af44ad5282594c7751d25422256a4f48fb09f3284fa692d` |
| [`server/api/tso.go`](../../../server/api/tso.go) | 75,76 | `f449fac9e1413a5e9c698e330f9c0dba9877dace0ae5a3aba7c22c3ea8d8db9e` |
| [`server/api/unsafe_operation.go`](../../../server/api/unsafe_operation.go) | 102 | `c5236b9f276e8a6d0c199031c2bee8931dc3d96b72e53451886fa841f25609f2` |
| [`server/api/unsafe_operation_test.go`](../../../server/api/unsafe_operation_test.go) | 102 | `ced4b04b938914db4c368143a6cff170bb575fde0b35512a811fe4848f68d290` |
| [`server/api/version.go`](../../../server/api/version.go) | 46 | `dbd70edd160452906c8ecddcb4b324b2b8296d585e15b286d2e60833c3c85d6b` |
| [`server/apiv2/handlers/affinity.go`](../../../server/apiv2/handlers/affinity.go) | 2,71,72,78 | `1ab2786431b6553402ec3eb984b5b367ab76a7aa43d08330a1434ad532e31f2d` |
| [`server/apiv2/handlers/keyspace.go`](../../../server/apiv2/handlers/keyspace.go) | 2,78 | `95d941e35cdb3191330a3f44ca8a84843f5f350afe5c1fdd872ef5f8d552ba6b` |
| [`server/apiv2/handlers/maintenance.go`](../../../server/apiv2/handlers/maintenance.go) | 2,78 | `31ee3d9df5a11e89e7f8fbc7010239c7338273509949ef377123ff6c81b91642` |
| [`server/apiv2/handlers/meta_service_group.go`](../../../server/apiv2/handlers/meta_service_group.go) | 2,78 | `1f8f9cf5d2fcdbbdae528c9a0c84f7d8cdda928b77e83212d26bccf37b06c3f9` |
| [`server/apiv2/handlers/microservice.go`](../../../server/apiv2/handlers/microservice.go) | 2,78 | `b41a26c789057f2b252298b82f5f7489aca7ee88e6a4c4c231a762b2c476df60` |
| [`server/apiv2/handlers/microservice_test.go`](../../../server/apiv2/handlers/microservice_test.go) | 2,78 | `db5a3c3985fe0c19d4bc7221d671cdb04c202e257b6738b9571d0c62ca056993` |
| [`server/apiv2/handlers/ready.go`](../../../server/apiv2/handlers/ready.go) | 2,43,78,94 | `1c26fd1694d3e1c0124a6f0c9d7b212187caa9b55856e74dd2ae574ad0c97ebc` |
| [`server/apiv2/handlers/safe_point.go`](../../../server/apiv2/handlers/safe_point.go) | 2,78 | `9079855d64ac03b114332ca172a9781304ced877516d25bf7364627da214ceb5` |
| [`server/apiv2/handlers/tso_keyspace_group.go`](../../../server/apiv2/handlers/tso_keyspace_group.go) | 2,78 | `7c288c5eb843f484f22f1c59db441a2f43fe3ffdbff351b9f658d998d8382aaa` |
| [`server/apiv2/middlewares/bootstrap_checker.go`](../../../server/apiv2/middlewares/bootstrap_checker.go) | 102 | `98d796c4fbdd5bb71daa4bf6772ff038ad34320128611fbe7b920c8f61334feb` |
| [`server/apiv2/middlewares/microservice_redirector.go`](../../../server/apiv2/middlewares/microservice_redirector.go) | 102 | `5cb2b2e990f19f8877443365ca16fc8e4aae77cad82c49e5a8b81d80363f7c39` |
| [`server/apiv2/middlewares/redirector.go`](../../../server/apiv2/middlewares/redirector.go) | 41 | `cc1eb102921ca3a1115c515a908599f0023160bb4bc1f6b24c2fefa346f28da5` |
| [`server/apiv2/router.go`](../../../server/apiv2/router.go) | 2,78,101 | `c6661e0e949be8eb7fdf4823991f4456902c3b02129317352e522b2daf242837` |
| [`server/cluster/Cargo.toml`](../../../server/cluster/Cargo.toml) | 1,27,34,77 | `760306898438fe850c300dc9a975f60677e731eed2fef4b269040b59235c5184` |
| [`server/cluster/bucket_heartbeat.rs`](../../../server/cluster/bucket_heartbeat.rs) | 1,10,27,34,77 | `31e5099cd2fcbae1dbc47a1cea9be5ac64503183f02a0fafdb8d56eea308f8c9` |
| [`server/cluster/bucket_heartbeat.rs.md`](../../../server/cluster/bucket_heartbeat.rs.md) | 1,27,34,77 | `d9850a5ffae793c8f9a7edae617ab9b3aef9f8d8bf96d1ec3952c8f6c93f1516` |
| [`server/cluster/bucket_heartbeat_test.rs`](../../../server/cluster/bucket_heartbeat_test.rs) | 1,27,34,77 | `b77408b57c3f559b353fe64ea44b740dacde9fe7848a2d7f16d9ac8cc8052bd8` |
| [`server/cluster/cluster.go`](../../../server/cluster/cluster.go) | 1,6,12,13,14,15,16,20,27,34,77 | `a6bc285063b31d56ce4db42bf26f9409144ea35d05a90a7f0dace98a8add291e` |
| [`server/cluster/cluster_test.go`](../../../server/cluster/cluster_test.go) | 1,27,34,77 | `11c9f4767ab374ee4920d7a54e4411c031afbec9be61af191102e0b8cea4f71d` |
| [`server/cluster/cluster_worker.go`](../../../server/cluster/cluster_worker.go) | 1,27,34,77 | `3d43809d617072b5bb58f5bbae99436de457aa6e889a75c5b75e215a55523a22` |
| [`server/cluster/cluster_worker_test.go`](../../../server/cluster/cluster_worker_test.go) | 1,27,34,77 | `f3606524d35852a17828f3ab6bc7fdbbc55821fab116c3e01a823c9322a5bcbe` |
| [`server/cluster/heartbeat_effects.rs`](../../../server/cluster/heartbeat_effects.rs) | 1,9,27,34,77 | `45eac087f2d116ab1e71a9f7295157af947436a99ad5d2c7e1f38c5865154cf0` |
| [`server/cluster/heartbeat_effects.rs.md`](../../../server/cluster/heartbeat_effects.rs.md) | 1,27,34,77 | `9f5076677ef72ccdbe7a7da8da5a63fe70f9293ae6f07a6a17b75dc507e42961` |
| [`server/cluster/heartbeat_effects_test.rs`](../../../server/cluster/heartbeat_effects_test.rs) | 1,27,34,77 | `7213fcf77b16a6423f1e3054a01f0e844e3757bfa32119ea8e29ea708735920d` |
| [`server/cluster/lib.rs`](../../../server/cluster/lib.rs) | 1,27,34,77 | `df45f7825a3d44ebad6c5e3c827be3e96593ff5b91653e55b811854ca92f9b1c` |
| [`server/cluster/lib.rs.md`](../../../server/cluster/lib.rs.md) | 1,27,34,77 | `4125f8b28f54a1d7d710b22d2bfa9f0de24173376457575a9dbcea8ae0c7dabd` |
| [`server/cluster/metering.go`](../../../server/cluster/metering.go) | 1,27,34,77 | `9fcb58e013c61d4716f14c3f2f1adbd6b5c575fd8d1f9b3b472faacae2dbe1ab` |
| [`server/cluster/metering_test.go`](../../../server/cluster/metering_test.go) | 1,27,34,77 | `399580dd243e271bf93bf710b3ad81ed8e8e60072f6c9edf75108dea9f0fc257` |
| [`server/cluster/metrics.go`](../../../server/cluster/metrics.go) | 1,27,34,77 | `d05081e40e6708ad169f5e6424dc53a9842806099f5426f50b4adbee30502183` |
| [`server/cluster/region_heartbeat.rs`](../../../server/cluster/region_heartbeat.rs) | 1,27,34,77 | `77286dc4a74d821562e4bc3036f2288f5c8cae248550ba84f1691362d9784803` |
| [`server/cluster/region_heartbeat.rs.md`](../../../server/cluster/region_heartbeat.rs.md) | 1,27,34,77 | `ad34ca3cc31de5697b246754832d04d4427b69d5db4fdc59d8561d0d08dc1e8f` |
| [`server/cluster/region_heartbeat_test.rs`](../../../server/cluster/region_heartbeat_test.rs) | 1,27,34,77 | `2dda4b2b8cea1b27615eccdfa7effc4cd0cc7e033117ac2e8db8e49b361ebf22` |
| [`server/cluster/region_overlap.rs`](../../../server/cluster/region_overlap.rs) | 1,27,34,77 | `a3bfd9177b9adf549ea72107ef8db81a1319d208c5c408d4840bbbc6a6632637` |
| [`server/cluster/region_overlap.rs.md`](../../../server/cluster/region_overlap.rs.md) | 1,27,34,77 | `932a2a824a57894c37d969a50fee4da6ada95baa3abf9714095a12b92234992f` |
| [`server/cluster/region_overlap_test.rs`](../../../server/cluster/region_overlap_test.rs) | 1,27,34,77 | `d823184bb98d28a66c1e3ce2b6a99b708d04c0b11239726d37bd89f76ec7afe2` |
| [`server/cluster/removed_store.rs`](../../../server/cluster/removed_store.rs) | 1,27,34,77 | `a1a58ad0eaf19138dbace800099c494274ba60e8e874353ce9c1fcf39dd41ab4` |
| [`server/cluster/removed_store.rs.md`](../../../server/cluster/removed_store.rs.md) | 1,27,34,77 | `a3adad2f8539c58bd3daaad9f1e0a19cd880ec4e29779929e9e7c24bbe3c7804` |
| [`server/cluster/removed_store_test.rs`](../../../server/cluster/removed_store_test.rs) | 1,27,34,77 | `d42089f1fdd7bfb7c9fff6c3a088fd017162ed8c41de9ca0466a285c019836f3` |
| [`server/cluster/scheduling_controller.go`](../../../server/cluster/scheduling_controller.go) | 1,27,34,77 | `210d8ff39676da2fa94a0cfa15b37049b0f667ab8d2045bfbaa4a14559a4d678` |
| [`server/cluster/split_scatter_test.go`](../../../server/cluster/split_scatter_test.go) | 1,27,34,77 | `3ff88b0338ac2f14d865ecc9fefd6891bbac2db378db701881fc93da1393fb2e` |
| [`server/cluster/state_pipeline.rs`](../../../server/cluster/state_pipeline.rs) | 1,5,6,12,13,14,15,16,27,34,77 | `53122deaad6db8871bda7234ef162ffadc40bb58c864292c0b3f4e1aa595ecae` |
| [`server/cluster/state_pipeline.rs.md`](../../../server/cluster/state_pipeline.rs.md) | 1,27,34,77 | `a30ea018ad8bca0e19c15dca1fc8753eae0bb0977a2c1918516a5bc555745f00` |
| [`server/cluster/state_pipeline_test.rs`](../../../server/cluster/state_pipeline_test.rs) | 1,27,34,77 | `97c69c9541cf9f55f95839c4a28bc1d23f2c32e3bf6018e9ed680d5a0834639f` |
| [`server/cluster/store_heartbeat.rs`](../../../server/cluster/store_heartbeat.rs) | 1,27,34,77 | `886cb6292814ddd893ca56722dd82a482218c02f27d18bdf21edcb3b20148346` |
| [`server/cluster/store_heartbeat.rs.md`](../../../server/cluster/store_heartbeat.rs.md) | 1,27,34,77 | `6be2f6defd069ee7d278df8759656749c59e81ef237df5c36deab60a87804027` |
| [`server/cluster/store_heartbeat_test.rs`](../../../server/cluster/store_heartbeat_test.rs) | 1,27,34,77 | `d6fec120e0ac2c6e94c797f8a690208b7c034937044a4cff46a76f340cbbe64c` |
| [`server/config/Cargo.toml`](../../../server/config/Cargo.toml) | 3,28,47,83,84,85,90 | `55cddfb8f5351267918fc10eb63d7f6093843b4302f1852efc57214b2d3f59b5` |
| [`server/config/OWNERS`](../../../server/config/OWNERS) | 3,28,47,83,84,85,90 | `deb1cc78361d085f4d8bb1f467b560a608c194abfbaaaa188026ce23f9ac1bae` |
| [`server/config/config.go`](../../../server/config/config.go) | 3,28,47,83,84,85,90 | `fe6565d6d88cebbec8171adea49bba688762be8b32b2884ed4220d3c4d6448f8` |
| [`server/config/config_decode.rs`](../../../server/config/config_decode.rs) | 3,28,47,83,84,85,90 | `91d8cac060a2e65e504f3a8f3b80bbacabcd3caab3a7edfe062809d1c907d8da` |
| [`server/config/config_decode_test.rs`](../../../server/config/config_decode_test.rs) | 3,28,47,83,84,85,90 | `46ddce324b8380720f765600ed3754a082f39d4abdd546afc9ef78be4122fe29` |
| [`server/config/config_defaults.rs`](../../../server/config/config_defaults.rs) | 3,28,47,83,84,85,90 | `a8e30e649259de316ca53da1b97b1a06cee3d429e07bc75d14fde03ae7ba2f93` |
| [`server/config/config_defaults_test.rs`](../../../server/config/config_defaults_test.rs) | 3,28,47,83,84,85,90 | `ce443f51864c7ba3dcb42bed46e932c56b3a736024cecb13e5b2d3c0879515cb` |
| [`server/config/config_hot_update_test.rs`](../../../server/config/config_hot_update_test.rs) | 3,28,47,83,84,85,90 | `f473cd1fd38be5062551cb9788f8b74cb2b3fba3b18bd6c00c85bd5867408f92` |
| [`server/config/config_persist_test.rs`](../../../server/config/config_persist_test.rs) | 3,28,47,83,84,85,90 | `e99b23a88c8881f5f5bd5477777d91cf1f4cad7ad27adfd3e3fbec3ae0a501ad` |
| [`server/config/config_test.go`](../../../server/config/config_test.go) | 3,28,47,83,84,85,90 | `705c57ffdbd953a5e9c274a1fe7cc31db8b6ebdbf31e4493f798242a245a847f` |
| [`server/config/config_tso.rs`](../../../server/config/config_tso.rs) | 3,28,47,83,84,85,90 | `013046025d613351814435d28d95800aa2bd849f3e556f3f56e4c00c519163c9` |
| [`server/config/config_tso_test.rs`](../../../server/config/config_tso_test.rs) | 3,28,47,83,84,85,90 | `57e8b5775341e646b4dcc3251158708e86a3f74b77de7c017df28da74138b657` |
| [`server/config/config_urls.rs`](../../../server/config/config_urls.rs) | 3,28,47,83,84,85,90 | `5a7fcd732d746400b5f264142f86196c08276bcdb586e2d0cff8a2bd825cf2c5` |
| [`server/config/config_urls_test.rs`](../../../server/config/config_urls_test.rs) | 3,28,47,83,84,85,90 | `b486d35a5a12d721f7d76c62ab3fd5f23a5c92cf1deb8b7f6ef8f326f208149c` |
| [`server/config/dashboard_config.rs`](../../../server/config/dashboard_config.rs) | 3,28,47,83,84,85,90 | `6adbfa24c9755ab7048121c4b5c8e243cc55686a44c48f5c23c1620a1e6adead` |
| [`server/config/dashboard_config_test.rs`](../../../server/config/dashboard_config_test.rs) | 3,28,47,83,84,85,90 | `77b9fd3bf776aad6211cad039ca908c471554a2ef6893ca91249998f7d58c6bf` |
| [`server/config/deprecated_config.rs`](../../../server/config/deprecated_config.rs) | 3,28,47,83,84,85,90 | `49e19a9153f7782b6f49f4aa4fd4bd5bbc9593649f124f9ebe7e93fa36239822` |
| [`server/config/deprecated_config_test.rs`](../../../server/config/deprecated_config_test.rs) | 3,28,47,83,84,85,90 | `39fdcc43a7a244b81338e3963c28adae25f91dce02445d1887e537243122571a` |
| [`server/config/fixtures/nested_config_go.json`](../../../server/config/fixtures/nested_config_go.json) | 3,28,47,83,84,85,90 | `c93d600dc32de9a62a2496ba8dce4a949270941cd1529b5417a45969a48ba7b8` |
| [`server/config/keyspace_config.rs`](../../../server/config/keyspace_config.rs) | 3,28,47,83,84,85,90 | `ffa2e214b1a1814f68b827c0de122f851636b7158ec2a0aff79d62f6dee5fc7d` |
| [`server/config/keyspace_config_test.rs`](../../../server/config/keyspace_config_test.rs) | 3,28,47,83,84,85,90 | `3686fbae59ca1477412640527a7835042f9e0e0bae6c8b2cedeacbb03811d3b3` |
| [`server/config/lib.rs`](../../../server/config/lib.rs) | 3,28,47,83,84,85,90 | `6f16860bb1833f5bc701781119444b203d45100214681e69978060f1d4404b65` |
| [`server/config/metrics.go`](../../../server/config/metrics.go) | 3,28,47,83,84,85,90 | `709d594be4d7b2233cf4804e2080c234b3f546ba34898692e2f5ca3b78586d37` |
| [`server/config/microservice_config.rs`](../../../server/config/microservice_config.rs) | 3,28,47,83,84,85,90 | `63cfe4c91556e3ac0c7589030047f93915869594b3a85748ae1980623bbedef4` |
| [`server/config/microservice_config_test.rs`](../../../server/config/microservice_config_test.rs) | 3,28,47,83,84,85,90 | `7e44f823217a26d58f483703ec7dfc18a7519d547b939f180f04248d3102dd1d` |
| [`server/config/middleware_config.rs`](../../../server/config/middleware_config.rs) | 3,28,47,83,84,85,90 | `f502b00bb6a68e5413f160e1555f0b45b36630f6bb8dfdff7b29a7c34aaf2300` |
| [`server/config/middleware_config_test.rs`](../../../server/config/middleware_config_test.rs) | 3,28,47,83,84,85,90 | `b457ce0e53b81715eb086185c4647aab16dd297968176b4c4976530e4131d3bd` |
| [`server/config/nested_config.rs`](../../../server/config/nested_config.rs) | 3,28,47,83,84,85,90 | `17d6adb2c7fc4e357239c49006b50e4235d350d7cdba7ebafbd37e9aa3231694` |
| [`server/config/nested_config_test.rs`](../../../server/config/nested_config_test.rs) | 3,28,47,83,84,85,90 | `5a23694872985c4b0e477ac14de389482a52568ca6a31b04e6bf0f34d1c9bb8d` |
| [`server/config/pd_server_defaults.rs`](../../../server/config/pd_server_defaults.rs) | 3,28,47,83,84,85,90 | `caf63a77229b60ebd706efed96927f2d12259c54c2a012babb501b7ec4760ed6` |
| [`server/config/pd_server_defaults_test.rs`](../../../server/config/pd_server_defaults_test.rs) | 3,28,47,83,84,85,90 | `e9ddb6a64ca0d0eae09936701690485fb28846b5a21ae61fa2eeede23e55372c` |
| [`server/config/pd_server_gc.rs`](../../../server/config/pd_server_gc.rs) | 3,28,47,83,84,85,90 | `f9bf88052d7129b736076f13eeeccbc662d15539097240243a6483386eda2a40` |
| [`server/config/pd_server_gc_test.rs`](../../../server/config/pd_server_gc_test.rs) | 3,28,47,83,84,85,90 | `f36d1bea8c7a0edc76dcfae50793cd2fb8bd82009569dc665f750d78800d0ce3` |
| [`server/config/pd_server_validate.rs`](../../../server/config/pd_server_validate.rs) | 3,28,47,83,84,85,90 | `50438eeeef827f0715e29aaac0e3c8c6b5f0424cc3758f8ab8e8988162c94f6b` |
| [`server/config/pd_server_validate_test.rs`](../../../server/config/pd_server_validate_test.rs) | 3,28,47,83,84,85,90 | `fb6583c62c3818ebaf32782364a32ac4bf93c35b621e5e4bee2c8a46e79239ff` |
| [`server/config/persist_options.go`](../../../server/config/persist_options.go) | 3,28,47,83,84,85,90 | `d49284b2eee7f9a509f32598ed69fe47bdccda674cb4940d80c2ac7ad10b62ff` |
| [`server/config/persist_schedule.rs`](../../../server/config/persist_schedule.rs) | 3,28,47,48,49,50,83,84,85,90 | `9df9cb22edfcf76193109fe1ef4a298006ae747b53e5821e221770fc65eaee0d` |
| [`server/config/persist_schedule.rs.md`](../../../server/config/persist_schedule.rs.md) | 3,28,47,83,84,85,90 | `5102df166dceaace77d73b52a24770184ba4b9e4cb041d6e97d30bbaafbbb704` |
| [`server/config/replication_config.rs`](../../../server/config/replication_config.rs) | 3,28,47,83,84,85,90 | `a829afc2562b3d0f37afb62d1e7b1adbca67fa46229d32a9c241eb13a4f7f00f` |
| [`server/config/replication_config_test.rs`](../../../server/config/replication_config_test.rs) | 3,28,47,83,84,85,90 | `e1962abcb5875ca6c20de346fea3ca0d2fb22d198ca9d6fbf54846f3adc863f3` |
| [`server/config/service_middleware_config.go`](../../../server/config/service_middleware_config.go) | 3,28,47,83,84,85,90 | `bf5fcd9ef17ac7810e84b2f2447c20178f6ff6c12dc7537e91a6f83d2d2399b1` |
| [`server/config/service_middleware_persist_options.go`](../../../server/config/service_middleware_persist_options.go) | 3,28,47,83,84,85,90 | `f14e3648c04aba2723ba0c10f53a60811fc9e65b7b84d094fa4f954e15ea8f5e` |
| [`server/config/util.go`](../../../server/config/util.go) | 3,28,47,83,84,85,90 | `503276291dee3b7811b0dad72b7a1b95e3a8cc76a79fd3b208ec952072c139fc` |
| [`server/config/util_test.go`](../../../server/config/util_test.go) | 3,28,47,83,84,85,90 | `c9d87606ed8d565288206e89058375e227da65036810bfa53d67373b91b039d0` |
| [`server/grpc_service.go`](../../../server/grpc_service.go) | 2,8,11,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,42,73,74,100 | `16fe2a147fe3d34966ca08c59c429871bf56242879839b56ded50af34e8dc088` |
| [`server/join/client_url.go`](../../../server/join/client_url.go) | 39,88 | `3c97f176b702a86d229080db04eadf395dcc85ec1a25b8b0a31c311c3e33d20e` |
| [`server/join/client_url_test.go`](../../../server/join/client_url_test.go) | 102 | `ab758893a1fcca5ff66e64faab8231d37e0e653d7569c8e7368ae0d883d8bc07` |
| [`server/join/join.go`](../../../server/join/join.go) | 88,89 | `1a4a91520a2d522339a1a2e0d842bdcc5cad8515b69be4efd7c366bffd9bc36b` |
| [`server/metrics.go`](../../../server/metrics.go) | 86 | `76eadd41bc68d62b19605ca4e2578e176311aebb2f8d72d50e39b26a5c62917f` |
| [`server/server.go`](../../../server/server.go) | 1,7,82,94 | `829eb592771c69f68f0c5efb8b6a88565828c3b0d5430d01c8ec60948fb2ad80` |
| [`tests/rust-compat/Cargo.toml`](../../../tests/rust-compat/Cargo.toml) | 100,101 | `8d7435c1f2b9395d49ccdd6391ea35c0f026dc2fb71a42980eb3bbdba2e33d3d` |
| [`tests/rust-compat/README.md`](../../../tests/rust-compat/README.md) | 100,101 | `d0e30116c65e9247a0b1aa9f7c62b72ff3a938a74604b23587a61f41ad4e418f` |
| [`tests/rust-compat/fixtures/different_service.rs`](../../../tests/rust-compat/fixtures/different_service.rs) | 100,101 | `b46e9de70a99513f6ef014bbbb56fc983311f1067da83053629eab1255b69146` |
| [`tests/rust-compat/fixtures/health-member.json`](../../../tests/rust-compat/fixtures/health-member.json) | 100,101 | `87beba0cf1c80eba8426aa1304ff158957679e536308a1467857d2569c7485f4` |
| [`tests/rust-compat/fixtures/state/task-01.json`](../../../tests/rust-compat/fixtures/state/task-01.json) | 100,101 | `54d06eb716a285ce6b40913484cda2c70f1bba1ff7ae194789c2261d507f3434` |
| [`tests/rust-compat/fixtures/state/task-04.json`](../../../tests/rust-compat/fixtures/state/task-04.json) | 100,101 | `b4c09c8e1eb975b80df913799b81440c5cdab259827b54e685f5e9b0c5b7dd9f` |
| [`tests/rust-compat/fixtures/state/task-25.json`](../../../tests/rust-compat/fixtures/state/task-25.json) | 100,101 | `ee89d3f6b020f6f21c454a6aa6acf82e6443319e01d63ed6d220387a50ca1337` |
| [`tests/rust-compat/fixtures/state/task-26.json`](../../../tests/rust-compat/fixtures/state/task-26.json) | 100,101 | `221625fa42b44c081645c5f0fcb54458c411e3e1457596ba80e4cb0d13d5c411` |
| [`tests/rust-compat/fixtures/state/task-27.json`](../../../tests/rust-compat/fixtures/state/task-27.json) | 100,101 | `cdd083f624f797d8978755ce1df96759ad524144d04f973b0452feb9cacd7ac0` |
| [`tests/rust-compat/fixtures/state/task-28.json`](../../../tests/rust-compat/fixtures/state/task-28.json) | 100,101 | `cc7c346ddc89efee85c5f34dd8832ea4e97a03a49e70c48ee6689961dc43bebb` |
| [`tests/rust-compat/fixtures/state/task-30.json`](../../../tests/rust-compat/fixtures/state/task-30.json) | 100,101 | `f8a45e96fa1856273e751533d1522ec55416f2981a0281a89578af9c226b5d44` |
| [`tests/rust-compat/fixtures/state/task-50.json`](../../../tests/rust-compat/fixtures/state/task-50.json) | 100,101 | `af43b34ffc6c06a1d5df88d5b9c6c0a18815e6e661dd2f39c48176be53f54db7` |
| [`tests/rust-compat/fixtures/state/task-53-restart.json`](../../../tests/rust-compat/fixtures/state/task-53-restart.json) | 100,101 | `3c72780024caeaa1ba8b57235d716979e5ac1039562091055687d7b46801781a` |
| [`tests/rust-compat/fixtures/state/task-53.json`](../../../tests/rust-compat/fixtures/state/task-53.json) | 100,101 | `d6b70e0facb12aa2696c0914a1bb1f35078a0af1a452bee7c92e7feb8ad735dc` |
| [`tests/rust-compat/harness.rs`](../../../tests/rust-compat/harness.rs) | 4,98,99,100,101 | `6190a2b103fe5292750dd0a3263812e91747bc4c329000d6987d88779b0a2a26` |
| [`tests/rust-compat/health_member_test.rs`](../../../tests/rust-compat/health_member_test.rs) | 100,101 | `4e3447b170faa55e409b5430fb39488072b20e5fa2c4aae370b64240d1fcecba` |
| [`tests/rust-compat/lib.rs`](../../../tests/rust-compat/lib.rs) | 100,101 | `5210ba1e794d6367b94b8aa8a79c331bb2540f90bbd3248b1df90fbebf0b6316` |
| [`tests/rust-compat/normalize.rs`](../../../tests/rust-compat/normalize.rs) | 100,101 | `c9f4743d14060b0b51461dc6ae87b446b0e88e5881620e72e400ab5a5d7aa013` |
| [`tests/rust-compat/schedule_acceptance_test.rs`](../../../tests/rust-compat/schedule_acceptance_test.rs) | 100,101 | `04c10e4ac02cf7b9645e95dae7609a1dbbd108dfcc20f0dd3fbf538db6be43a4` |
| [`tests/rust-compat/schedule_foundation.rs`](../../../tests/rust-compat/schedule_foundation.rs) | 100,101 | `6291868a94e7622f11dbdb4f0f0f7ba6dc66d2acf782e8f12d0b872543badfa8` |
| [`tests/rust-compat/schedule_foundation.rs.md`](../../../tests/rust-compat/schedule_foundation.rs.md) | 100,101 | `bf5dae111f72e9f9f658f1c7a25bafb14d07c9600fe9272a53a0a6927f20cc98` |
| [`tests/rust-compat/schedule_foundation_test.rs`](../../../tests/rust-compat/schedule_foundation_test.rs) | 100,101 | `3d899fc6dcea7ad672ac134e6b81ccf7983361b65ad8b297f5f19e978b96683f` |
| [`tests/rust-compat/schedule_loop_test.rs`](../../../tests/rust-compat/schedule_loop_test.rs) | 100,101 | `11bffdef1390285659be28c86c326ecabca3c6ed354c2c5acec327b95d55ef1d` |
| [`tests/rust-compat/schedule_performance_test.rs`](../../../tests/rust-compat/schedule_performance_test.rs) | 100,101 | `91317ea6bdf479595ca4608c382cd11dbefbe8d7d032ecddb149edf48523948c` |
| [`tests/rust-compat/schedule_restore_test.rs`](../../../tests/rust-compat/schedule_restore_test.rs) | 100,101 | `b68f046c931b69f702d3934a59c83fc6f436176aa5dfe86df9c08c1760dfb75d` |
| [`tests/rust-compat/state_oracle.rs`](../../../tests/rust-compat/state_oracle.rs) | 100,101 | `560e4ce3e2ff05fdf724b2282974dc42ee394c9bd0016827073fc9eab566a26e` |
| [`tests/rust-compat/state_oracle.rs.md`](../../../tests/rust-compat/state_oracle.rs.md) | 100,101 | `dd8342d4761d4a34a5a04b39eb0ce4cd1fe65f8fe47ac6ac3fdff3bcf05eb393` |
| [`tests/rust-compat/state_regression.rs`](../../../tests/rust-compat/state_regression.rs) | 100,101 | `c38e13a673a36906bc3e77af11129d79ecc003a705ecac5653054f2323b62be8` |
| [`tests/rust-compat/state_regression.rs.md`](../../../tests/rust-compat/state_regression.rs.md) | 100,101 | `f324554cb6d8a4059a775e64bc1df0af7341ae692df728a569d5ff684c978e15` |
| [`tests/rust-compat/state_regression_test.rs`](../../../tests/rust-compat/state_regression_test.rs) | 100,101 | `b0592632c03ad2a0f0aaf5e3eb270d4e35679a5d22bc0a9c15dc8700a0a073a6` |
| [`tests/rust-compat/tso_test.rs`](../../../tests/rust-compat/tso_test.rs) | 100,101 | `ad39b71fe0906d0af3e0647796be16b274dd4d7411bb0ca77ae3b6c0a42c8634` |
| [`tests/server/api/admin_test.go`](../../../tests/server/api/admin_test.go) | 99 | `19404389a80456b8789cd21ff9f914a7d89bd233be0a6c71b2fd4411d63d4e6d` |
| [`tests/server/api/api_test.go`](../../../tests/server/api/api_test.go) | 99 | `c142abe85c075a3e07d1c0e76a44516a8f9febabea25b905186293a8ee209f02` |
| [`tests/server/api/checker_test.go`](../../../tests/server/api/checker_test.go) | 99 | `a0769c8984335e77a0bed80f6c3beb92fed99b72dbc3dd722aa1963ef76fb91a` |
| [`tests/server/api/cluster_test.go`](../../../tests/server/api/cluster_test.go) | 99 | `bdf3703637760e0917fa97890c93f6d5cfcaed9e69552d7a171a9f5d0e01115c` |
| [`tests/server/api/diagnostic_test.go`](../../../tests/server/api/diagnostic_test.go) | 99 | `bae7afc37c339b12004126055d0a9513c182918dfb582261e4efa5294a803517` |
| [`tests/server/api/etcd_api_test.go`](../../../tests/server/api/etcd_api_test.go) | 99 | `137ddc78ac0d07edaf8d91ccd218d8a1fd377f66eba363818b6a91527f50bae2` |
| [`tests/server/api/health_test.go`](../../../tests/server/api/health_test.go) | 99 | `abc5fe02387ba5b321ab15b4ef67a978d5200c64d96076ba61e14798e2423435` |
| [`tests/server/api/hot_status_test.go`](../../../tests/server/api/hot_status_test.go) | 99 | `da1eff4f0d66851c8a2f95908b672a3fe6f9852243b55116d3c9d1b7d4a6be1a` |
| [`tests/server/api/label_test.go`](../../../tests/server/api/label_test.go) | 99 | `676f4c9e97fd34719a3b6121a64ddffd2a5a1a25b48a46765e4c06ffc1421806` |
| [`tests/server/api/log_test.go`](../../../tests/server/api/log_test.go) | 99 | `3e4bf8555ce0aa6797844de41b4fd4c29b595c5ecb6c556a82695618a6fd5566` |
| [`tests/server/api/member_test.go`](../../../tests/server/api/member_test.go) | 99 | `69a68e93f4c9f959500f3ccb99054a5e205a0c03d583311f53155b58ce7752aa` |
| [`tests/server/api/min_resolved_ts_test.go`](../../../tests/server/api/min_resolved_ts_test.go) | 99 | `42f0086ce3e8eadc20818cc3e691833a74839a0d2c9af9909d26813d8ac73dc8` |
| [`tests/server/api/operator_test.go`](../../../tests/server/api/operator_test.go) | 99 | `98b05af58107a8f9326baf08af7a2f127c3a82fc4b5e5953741d9faabc70a3a2` |
| [`tests/server/api/pprof_test.go`](../../../tests/server/api/pprof_test.go) | 99 | `69f735719728f62a0cb34cfa29c2e074403c886ace71ea791f00147531b09ac5` |
| [`tests/server/api/region_label_test.go`](../../../tests/server/api/region_label_test.go) | 99 | `e96f1801ea354b3d5fbade000914bb190b297b6e0f93e1bb95948c936f1cc71a` |
| [`tests/server/api/region_test.go`](../../../tests/server/api/region_test.go) | 99 | `58f4e42206dbad919981c116dd703517bb8d2369227f4f92a5041a7f73e1412f` |
| [`tests/server/api/rule_test.go`](../../../tests/server/api/rule_test.go) | 99 | `25da942242dfe4c300d79a6621837c9f5082c9387634ba40835438a529133bb7` |
| [`tests/server/api/scatter_split_test.go`](../../../tests/server/api/scatter_split_test.go) | 99 | `5bc80e1f97ffe5988bcab0e039275cc2b81aebc524f9c92f81476479dc40de13` |
| [`tests/server/api/scheduler_test.go`](../../../tests/server/api/scheduler_test.go) | 99 | `9c8693249a95c8c083a8a6eed9d01ee8acc2e9b79ec43228ef1a39593474e6ff` |
| [`tests/server/api/server_test.go`](../../../tests/server/api/server_test.go) | 99 | `3f0e9f9d5a22b232a152b50fdcbcd46ac42f7579fc0423386646a47a9f30270e` |
| [`tests/server/api/service_gc_safepoint_test.go`](../../../tests/server/api/service_gc_safepoint_test.go) | 99 | `99022b54fa39f1c4a4959fb4eeb8145b9d29285941ddf2b4ac2da328e7724d98` |
| [`tests/server/api/service_middleware_test.go`](../../../tests/server/api/service_middleware_test.go) | 99 | `bc9c6869a70357c484d63a6077753f05a99610aae95db6330bf140f77ea7f026` |
| [`tests/server/api/stats_test.go`](../../../tests/server/api/stats_test.go) | 99 | `c476921f84f6f86fddfb0d25b96978a50b994d29e0e33c45fd31a71e5883e32c` |
| [`tests/server/api/status_test.go`](../../../tests/server/api/status_test.go) | 99 | `a45073f07f8636b754a135fb3c41dc2bc73ed1a6cf72d00c419deeb318a7ebae` |
| [`tests/server/api/store_test.go`](../../../tests/server/api/store_test.go) | 99 | `403eeb00cf4c4e55578701c5bed8de78b5732fe6166e96058b3518ae32b07724` |
| [`tests/server/api/unsafe_operation_test.go`](../../../tests/server/api/unsafe_operation_test.go) | 99 | `158559ff02bf0a92b8a5f553dda65f7153392eae3f0c92320f0d20b2dab522ec` |
| [`tests/server/api/utils.go`](../../../tests/server/api/utils.go) | 99 | `48fe2caa348efd40dff183eff91b433835e40a8aefb56bea35bada5a5fde10f8` |
| [`tests/server/api/version_test.go`](../../../tests/server/api/version_test.go) | 99 | `f6105ca2819160d9190ff76a28fe57509dedb8d5aad2be37cb79031a4c213c58` |
| [`tests/server/apiv2/handlers/affinity_test.go`](../../../tests/server/apiv2/handlers/affinity_test.go) | 99 | `65beeb7aa96490d6859990d78ed6123d834aaadc20e28b140cd784273659e929` |
| [`tests/server/apiv2/handlers/keyspace_test.go`](../../../tests/server/apiv2/handlers/keyspace_test.go) | 99 | `aa588436baa124041e067f981f4ea2ecd777ac7635b45157e106ef2c61327c86` |
| [`tests/server/apiv2/handlers/maintenance_test.go`](../../../tests/server/apiv2/handlers/maintenance_test.go) | 99 | `4e2ee2bf63ab65288ae181fdeb16d97ba2ac0bb2982cabeff69f4a9ddd9ef9ef` |
| [`tests/server/apiv2/handlers/meta_service_group_test.go`](../../../tests/server/apiv2/handlers/meta_service_group_test.go) | 99 | `7c5fd2f60841a9b5f47b6f76418644690061d60f478b51675fc77c0ffc4d6a56` |
| [`tests/server/apiv2/handlers/ready_test.go`](../../../tests/server/apiv2/handlers/ready_test.go) | 99 | `fec9b31ec6d14b77af06e8b6848a3093772d9aa20de18152d5577d9bbdf733e3` |
| [`tests/server/apiv2/handlers/safe_point_test.go`](../../../tests/server/apiv2/handlers/safe_point_test.go) | 99 | `83b9dae90e30769f56db1e2611d6bbb61887b5be833f215d8122b6560a04fc91` |
| [`tests/server/apiv2/handlers/testutil.go`](../../../tests/server/apiv2/handlers/testutil.go) | 99 | `db22b121dd8fa3b80c9cf49683ed1a5195242b89b2a0b78825e8adeb5350a93f` |
| [`tests/server/apiv2/handlers/tso_keyspace_group_test.go`](../../../tests/server/apiv2/handlers/tso_keyspace_group_test.go) | 99 | `f8d04a63ada6c3f67f841c0c40580436a33a49d25dd2a07412f6236d6bee840a` |
| [`tests/server/cluster/cluster_test.go`](../../../tests/server/cluster/cluster_test.go) | 99 | `df21a0e8db96c94ea64bdaf2719ef2685cdd0f952399e7124e7ea1094a879313` |
| [`tests/server/cluster/cluster_work_test.go`](../../../tests/server/cluster/cluster_work_test.go) | 99 | `7c010d7f3d27b02939ab379ec8ebf730a06d8ea1052e18ace189fdbe50c79625` |
| [`tests/server/cluster/cross_table_merge_test.go`](../../../tests/server/cluster/cross_table_merge_test.go) | 99 | `c16a73c402ef3a049a4b9d24c098ca0a26b5a6847dbd158976090940c128ad43` |
| [`tests/server/config/config_test.go`](../../../tests/server/config/config_test.go) | 99 | `f5592b21ce63bbd249c63babb9ada2d340389df4f5da6eeb2023e820cdccef2a` |
| [`tests/server/gc/gc_test.go`](../../../tests/server/gc/gc_test.go) | 99 | `2492d73403c6f9bfd41a909a91867276d2129c3202b66424e4223d1b283dac58` |
| [`tests/server/id/id_test.go`](../../../tests/server/id/id_test.go) | 99 | `acdd90c4685e08ce8ae887bf0cb326a1161a56955ee63f9939151a8e45bfb566` |
| [`tests/server/join/join_fail/join_fail_test.go`](../../../tests/server/join/join_fail/join_fail_test.go) | 99 | `be48a06466df535d96c01ce172326ada2a5869d1343e8488dd5baa91762077f4` |
| [`tests/server/join/join_test.go`](../../../tests/server/join/join_test.go) | 99 | `3333336eb36aabe289b6b54075a1012c3c7370a74cf645b46efe9155fa419ddb` |
| [`tests/server/keyspace/keyspace_service_test.go`](../../../tests/server/keyspace/keyspace_service_test.go) | 99 | `e45201b3c5c7fd98d6afeaf510b6b00b8913ac5cbd7dc6817eae3b0c04380f3a` |
| [`tests/server/keyspace/keyspace_test.go`](../../../tests/server/keyspace/keyspace_test.go) | 99 | `9cc47b312aa909697c64fe9273c5f9f60b906a94f5c253e64325893ff9b5038e` |
| [`tests/server/member/member_test.go`](../../../tests/server/member/member_test.go) | 99 | `c8078954f01908a35008ece3e621ebfed1e0f827da9d4bb808c67a6cadb9d485` |
| [`tests/server/member_leader_priority_metrics_test.go`](../../../tests/server/member_leader_priority_metrics_test.go) | 99 | `1653dd50340757b48bca423d2ea1e740fe0db5d40507b19f83f125b2f13545f3` |
| [`tests/server/region_syncer/region_syncer_test.go`](../../../tests/server/region_syncer/region_syncer_test.go) | 99 | `2c8b14fcf9f2512e41e24b40dee90ffc56557c615d4918fcbf099c3ff25c4433` |
| [`tests/server/server_test.go`](../../../tests/server/server_test.go) | 99 | `9e1cf2b98d8601154d77531542d4f5a0e1759ba0fe8be4d112adad376b07b0ab` |
| [`tests/server/storage/hot_region_storage_test.go`](../../../tests/server/storage/hot_region_storage_test.go) | 99 | `48b606632e4d6b674c028f7e84dc9532348e746757c48f0dcffc5b08f02a683d` |
| [`tests/server/tso/tso_proxy_test.go`](../../../tests/server/tso/tso_proxy_test.go) | 99 | `382e1c05292c5554419aa5ded0bd5524dbcd87452416d47a9eeb562ef6457a12` |
| [`tests/server/tso/tso_test.go`](../../../tests/server/tso/tso_test.go) | 99 | `7517352ca30e7e42ecffa3f276319beaf07c12af9d48b2f78bc994a92b734065` |
| [`tests/server/watch/leader_watch_test.go`](../../../tests/server/watch/leader_watch_test.go) | 99 | `80a60ac6b2d2ef7b0ca46712cba34e4c62271cb55beb5e243ddb12164858fc81` |
