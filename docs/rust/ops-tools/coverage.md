# 主题 9 最小覆盖矩阵

核对基准为 `79643c4ca94467d73d0ecf4ba56e34bc5b73d35f`，按
[运维 134 的既有验收范围](../../../.plans/2026-10-29-pd-rust-运维客户端与工具/134-主题九验收与缺口交接.md)
归集。**已运行**指存在可定位的实际结果，范围以回执为准。**待重跑**指入口已存在，
待直接前置满足后按正式场景运行。**真实缺失**指当前入口、装配或可用 oracle 缺失。
**需范围决策**指现有计划要求确认差异或替代边界。每行只对列出的子项给出相应状态。

| 运维任务 | 入口和已运行证据 | 剩余状态与解除条件 |
| --- | --- | --- |
| 1–3，134 覆盖、消费者、交接 | 本表和 [消费者矩阵](consumers.md) 固定现有入口及证据归属 | **需范围决策**：第 2 项 Go 专属运行时替代边界；第 134 项接收剩余正式证据。见 [准入交接](acceptance.md) |
| 4–5 版本与内核类型 | **已运行**：实际 release 的 CLI 与 HTTP 版本。[版本探针](task-132.md)、[Rust 内核类型入口](../../../pkg/versioninfo/kerneltype/lib.rs) | **待重跑**：正式构建身份注入和各已选内核类型契约。当前回执保留 Go/Rust 版本原值 |
| 6–10 审计与维护 | **已运行**：维护 API、WAL 和指标。[服务端 102](../server-api/task-102.md)。真实 [宿主路由](../../../pkg/runtime/node.rs) 接到 [审计中间件](../../../pkg/server/service_audit.rs) 及 [Prometheus sink](../../../pkg/server/service_metrics.rs) | **真实缺失**：完整发行所需本地审计日志 sink；**待重跑**：真实发行请求对日志及指标的观测 |
| 11–20 计量 | [Rust 配置](../../../pkg/metering/config/lib.rs)、[Go 计量入口](../../../pkg/metering) | **真实缺失**：实际收集、编码、持久 sink、重试及关停装配。先验证本地持久产物，再按任务覆盖对象存储及故障边界 |
| 21–27 资源与时钟 | Go [cgroup](../../../pkg/cgroup)、[内存](../../../pkg/memory)、[时钟监测](../../../pkg/systimemon) 仍可定位 | **真实缺失**：Rust 可观测路径与平台 fixture；**需范围决策**：任务 2/26 的 Go GC、finalizer、heap 和内存限制替代语义 |
| 28–45 客户端 | **已运行**：[Go client 对 Go/Rust 的切主和取消](task-045.md)。实际工具和现有 Rust 内部调用见 [消费者矩阵](consumers.md) | **真实缺失**：有工具消费者的 Rust 连接/发现、Region、TSO、Store、选用 GC/Keyspace 与 HTTP 路径；**需范围决策**：独立 MetaStorage/资源管理 SDK 扩展的消费者 |
| 46–85 pd-ctl | [Go HTTP 公共入口](../../../tools/pd-ctl/pdctl/command/global.go) | **真实缺失**：Rust CLI 和命令级 golden |
| 86–92 备份和恢复 | **已运行**：[备份](task-091.md)、[恢复](task-092.md)、[原始回执](task-091-092.json.gz)；含真实 etcd 持久状态、同源命令差分、TLS 和失败副作用 | **需范围决策**：确认 Go 备份空文件失败和重复恢复退出码差异在最终验收中的处置。复用既有行为回执 |
| 93–111 模拟器 | [Go 客户端与场景驱动](../../../tools/pd-simulator/simulator) | **真实缺失**：Rust 独立和嵌入入口，真实心跳与场景 golden |
| 112–116 导出和诊断 | [Region 导出](../../../tools/regions-dump/main.go)、[Store 导出](../../../tools/stores-dump/main.go)、[日志分析](../../../tools/pd-analysis/analysis) | **真实缺失**：Rust 工具与分页、日志及输出 golden |
| 117–124 benchmarks | **已运行**：三个现有 Go 工具实际消费 Region、TSO、心跳。[运维 45](task-045.md) | **真实缺失**：四个 Rust benchmark 入口及各自的参数、取消和报告 golden |
| 125–131 Swagger 和 Dashboard | **已运行**：默认 release 的禁用路由回执，Dashboard 配置检查见 [132](task-132.md)。[Rust 配置](../../../server/config/dashboard_config.rs)、[HTTP 回退](../../../pkg/server/http_server_config_write.rs) | **真实缺失**：Swagger 静态服务、完整 Dashboard backend 与 UI 资产接线；之后用真实服务验证代理、生命周期、资源改写及热力图 |
| 132–133 发行 | **已运行**：默认 release 构建、依赖图和本地进程行为。[运维 132 的局部回执](task-132.md) | **待重跑**：132 在 131 交接后按正式入口回归。**真实缺失**：完整装配和可用的完整 Go oracle，133 待实际指标、审计、计量、Swagger 和 Dashboard 行为证据 |
| 主题 8 上游 | **已运行**：[42 个注册/边界条目及运行索引](../control-plane-mcs/task-148.md)，含 TSO、Keyspace、GC 之外的扩展协议及真实服务宿主；[公共接口来源](../control-plane-mcs/admission.md) | **待重跑**：对照 134 实际消费范围接收主题 8 最终交接。服务侧协议回执与主题 9 客户端行为分开计入 |

既有回执保持各自的平台、配置、测试数和失败边界。主题 9 最终验收及生产 22/28 的顺序见
[准入交接](acceptance.md)。
