# 主题 9 消费者矩阵

基准为 `79643c4ca94467d73d0ecf4ba56e34bc5b73d35f`。表中 Go 调用定位工具迁移时的需求。当前 `client/` 无 Rust manifest，命令可交付状态以当前 Rust 二进制目标为准。已有 Go client 真实回归只覆盖其回执列出的调用。

| 实际入口 | 调用及数据通道 | 当前可验证结果与缺口 |
| --- | --- | --- |
| [pd-region-bench](../../../tools/pd-region-bench/main.go) | `NewClientWithContext`、GetRegion、GetPrevRegion、GetRegionByID；prepare 另走 RegionHeartbeat | 已运行 Go 查询和切主。Rust benchmark 与共享发现、连接、Region 点查路径缺失 |
| [pd-tso-bench](../../../tools/pd-tso-bench/main.go) | `NewClientWithAPIContext` 或按 keyspace 名构造；GetTS、批等待和 follower proxy 动态选项、超时与 Close | 已运行 Go TSO 及切主。Rust benchmark 与所需 TSO 调度、服务发现、取消及关闭缺失；具名 keyspace 路径在 [Go 构造函数](../../../client/client.go) 实际调用 LoadKeyspace |
| [pd-api-bench](../../../tools/pd-api-bench/main.go) 与 [注册用例](../../../tools/pd-api-bench/cases/cases.go) | HTTP Region 统计、min resolved TS；gRPC GetAllStores 初始化，配置可选 GetRegion、follower policy、ScanRegions、GetTS、GetStore 和两个 legacy GC 更新；KV 用例直连 etcd | 缺 Rust 工具和实际选用的 HTTP、Store、Region 范围及 legacy GC 请求路径。etcd KV 用例另接 etcd，未识别出此工具消费 PD MetaStorage |
| [pd-heartbeat-bench](../../../tools/pd-heartbeat-bench/main.go) | Go 生成的 PD stub 及 HTTP 准备；成员、bootstrap、store 注册、Region/Store 心跳 | 已运行既有 Go 心跳工具。Rust 工具缺失，按其直连协议和 HTTP 需求交接 |
| [pd-simulator](../../../tools/pd-simulator/simulator/client.go) 与 [驱动](../../../tools/pd-simulator/simulator/drive.go) | Go 生成的 PD stub，成员、ID 分配、bootstrap、store 注册及双心跳；HTTP 查 store | Rust 模拟器缺失。以真实协议和生命周期为验收对象，复用所需连接能力 |
| [pd-ctl](../../../tools/pd-ctl/pdctl/command/global.go) | PD HTTP client，端点选择、请求头和错误处理；[Region](../../../tools/pd-ctl/pdctl/command/region_command.go)、[Keyspace](../../../tools/pd-ctl/pdctl/command/keyspace_command.go) 命令走 HTTP | 缺独立 Rust 命令及所选 HTTP client 行为；分命令接受 golden |
| [pd-backup](../../../tools/pd-backup/pdbackup/backup.go) 与 [pd-recover](../../../tools/pd-recover/main.go) | Backup 读 etcd 和 PD HTTP config；Recover 写目标 etcd | 已运行同源 Go/Rust 命令和真实恢复；两 Rust 命令已有各自接线。差异见 [91](task-091.md) 和 [92](task-092.md) |
| [regions-dump](../../../tools/regions-dump/main.go)、[stores-dump](../../../tools/stores-dump/main.go) | 分页读 etcd，解码后导出文件 | 缺 Rust 导出命令及分页和文件 golden |
| [pd-analysis](../../../tools/pd-analysis/analysis/parse_log.go) | 读日志和诊断统计文件 | 缺 Rust 解析/统计命令及文件 golden |
| [pd-ut](../../../tools/pd-ut/README.md)、[utils](../../../tools/utils) | Go 测试驱动及工具共享辅助代码 | pd-ut 的 Go 专用执行器按计划作为 Go 来源保留；utils 按实际调用随对应工具交接 |

[运维 45](task-045.md) 已覆盖表中的 Go Region、TSO、心跳工具组合。当前实际 Rust 内部消费者包括
[Router 的成员与 Region 请求](../../../pkg/mcs/router/server/service_host.rs)、
[Scheduling 的 AllocID](../../../pkg/mcs/scheduling/server/remote_ids.rs)、
[Region 同步](../../../pkg/server/sync_transport.rs)和
[ResourceManager 转发](../../../pkg/server/grpc_resource_proxy.rs)。这些入口已直接使用协议 stub 或各自 adapter，可按其原有场景接收证据。

**确认有消费者的最小 Rust client 缺口**覆盖运维 28–39、41、43–44 中上述调用触达的行为。
优先用 Region benchmark 的成员发现和点查形成首个端到端交接，再以 TSO benchmark 验证流、取消和服务发现，
用 API benchmark 定位 Store、ScanRegions、两个 legacy GC 调用及其 HTTP 请求。Keyspace 先限于已触达的具名 TSO 初始化，
pd-ctl 的 Keyspace 管理走其真实 HTTP 路径。客户端缺口的验收应观察 Rust 工具实际消费这些入口。

运维 40 的 PD MetaStorage 及 42 的资源管理 SDK 扩展暂缺所选 Rust 工具调用方。现有
[Go ResourceGroupsController](../../../client/resource_group/controller/global_controller.go)
和 [主题 8 wire 回执](../control-plane-mcs/task-148.md) 分别提供 Go 内部调用和服务器协议来源。
更广的公开方法范围仍由原任务确认；当前记录保留 40/42 及 GC、Keyspace 未触达方法的交接项。
