# Rust 迁移依赖基线

本表记录当前仓库事实与已决定的替换方向，不是已安装的 Rust 依赖列表。
架构边界以 [ADR-001](architecture.md) 为准，验证流程见[移植指南](porting-guide.md)。
基线来自 [go.mod](../../go.mod)、[client/go.mod](../../client/go.mod)，记录于 2026-10-21。

| 当前 Go 依赖/能力 | 本地基线 | Rust 方向与约束 |
| --- | --- | --- |
| kvproto | 两模块均为 `v0.0.0-20261016101550-f8ad079c8e98` | AsterSQL kvproto fork 增加 prost/tonic 生成能力；固定可追溯 tag/commit |
| gogo/protobuf、grpc | v1.3.2、v1.82.1 | prost、tonic、tower；服务端仅使用新协议栈 |
| Gin、Gorilla、Negroni、cmux | v1.11.0、v1.7.4、v3.1.1、v0.1.5 | axum、hyper、tower；验证同监听器路由与探针 |
| etcd server/client/api | v3.5.15 | MetaStore + EtcdMetaStore；etcd-client 仅作过渡适配 |
| goleveldb | v1.0.1-0.20190318030020-c3a204f8e965 | trait 隔离；生产引擎等待 durability 原型，无预选 |
| zap、pingcap/log | zap v1.27.0（client v1.24.0） | tracing；统一字段、span、脱敏与宿主初始化 |
| pingcap/errors、errcode | errcode v0.3.0 | 叶子 thiserror，进程边界 anyhow；保留稳定错误码 |
| pingcap/failpoint | v0.0.0-20240528011301-b51a646c7c86 | Rust facade 与确定性注入；维护逐点迁移清单 |
| Prometheus client | v1.20.5 | 单一版本 prometheus、宿主 Registry；保持指标兼容 |
| BurntSushi/toml、Cobra、pflag | v1.5.0（client v0.3.1）、v1.8.0、v1.0.5 | serde/toml、clap；保留默认值、废弃项、退出码 |
| TLS | Go transport 配置 | rustls 默认；grpcio/OpenSSL 仅可留在过渡客户端边界 |
| AWS KMS/STS | v1.26.5/v1.31.1 | AWS Rust SDK；凭据链与敏感字段脱敏 |
| tidb-dashboard | v0.0.0-20261013052755-e5fc397a69b0 | 后期静态资源/API 适配，不阻塞核心 PD |
| Go .so scheduler plugin | Go ABI | 后期另定 RPC/WASM 边界，不承诺加载旧 ABI |
| goroutine、context | Go 运行时 | Tokio、显式取消；一个宿主运行时管理任务 |

## 版本与生成责任

Rust workspace 任务负责确认相互兼容的精确版本、features、工具链和最低支持版本，
统一在 workspace.dependencies 管理并提交 Cargo.lock；本任务不凭空指定 crate 版本。
协议任务负责记录 fork URL、不可变 commit、proto 来源、生成工具版本和生成命令，
审核字段编号、service、流和序列化兼容性，不允许跟随浮动分支。
当前未核验 AsterSQL fork 的版本或生成能力，不认定与本仓库已一致；协议任务须比较
本表 Go commit 与目标 fork，列出并解决漂移后再锁定。已有 Go client 必须继续可用。

HTTP/gRPC/TLS 的依赖特性应共同审查，避免引入第二套 protobuf、TLS 或异步运行时。
后续用 cargo metadata 和依赖树验证同一 prometheus 版本，不能仅检查 manifest。
协议生成工具与产物采用可重复生成策略，不让开发机器隐式下载浮动协议。

## 兼容基准入口

- 服务启动与 embedded etcd：[server/server.go](../../server/server.go)。
- 元数据行为：[pkg/storage](../../pkg/storage)、[pkg/member](../../pkg/member)。
- TSO：[pkg/tso](../../pkg/tso)；HTTP：[server/api](../../server/api)。
- 错误码：[pkg/errs](../../pkg/errs)、[errors.toml](../../errors.toml)。
- 独立入口：[cmd/pd-server/main.go](../../cmd/pd-server/main.go)。
- Go 集成测试：[tests/server](../../tests/server)；客户端：[client](../../client)。

计划中的 232 个 failpoint 是迁移盘点基线，不是本表重新核实的现时数量。
后续须扫描全部生产目录中的 Inject/InjectCall/其他注入形式，为每个点记录源码位置、
触发时机、故障语义、Rust 对应点及测试；新增/删除点要说明差异，不能靠数量认定覆盖。
