# Rust 迁移依赖基线

本表记录当前仓库事实与已决定的替换方向，不是已安装的 Rust 依赖列表。
架构边界以 [ADR-001](architecture.md) 为准，验证流程见[移植指南](porting-guide.md)。
基线来自 [go.mod](../../go.mod)、[client/go.mod](../../client/go.mod)，记录于 2026-10-21。

| 当前 Go 依赖/能力 | 本地基线 | Rust 方向与约束 |
| --- | --- | --- |
| kvproto | 两模块均为 `v0.0.0-20261016101550-f8ad079c8e98` | 当前精确消费 Go 同源 commit，在 OUT_DIR 生成 prost/tonic；未来 fork 切换须通过同一验收 |
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
AsterSQL fork 的 `v0.0.2-aster.20260929` 指向
`0e902bac627b0f2ef8955f2a28bf39838ec1b16c`，仍使用 grpcio/protobuf 2.8，
其 prost 0.9 feature 也依赖 grpcio，不能直接用于新服务端。
按任务继续执行授权，现采用以下同源生成策略，不等待 fork 发布；已有 Go client 继续可用。

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

## 当前协议生成基线

`astersql-pd-proto` 固定 pingcap/kvproto commit
`f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，与两个 Go 模块一致。
构建使用 Go 下载 `v0.0.0-20261016101550-f8ad079c8e98` 的模块 ZIP，校验完整归档
SHA-256 `aff09d074d493874bfd7e3e63c766a8d93f42901ff235dde2d2ced0c7f2a615e`，
然后仅在 Cargo OUT_DIR 解压与生成，不信任可编辑的模块解压缓存。
Go go.sum 中该版本的内容摘要为 `h1:ymP9CnhW3cNZLg5cea2ZgqCSTG91FrQts2e6HnULBu8=`。
不提交第三方源码、生成副本或 path patch；Cargo.lock 固定生成工具传递依赖。
构建需要 Go（遵循仓库 Go 版本）及首次可访问的 GOPROXY/Go 模块缓存，
无需系统 protoc；使用 protoc-bin-vendored 3.2.0 内的 protoc。
直接版本集中固定：prost/prost-types 0.14.1、tonic/tonic-prost/tonic-prost-build 0.14.2。
领域层仍禁止依赖协议类型；未来切换 fork 时须重新核验来源、摘要和 wire fixtures。

生成入口为 `pkg/proto/build.rs`。覆盖生产注册的 PD、Keyspace、Diagnostics、
ResourceManager、TSO、Scheduling、MetaStorage、Router 服务及传递消息依赖；
标准 gRPC Health 不属于 kvproto。注册位置见 `server/server.go`、
`pkg/mcs/{tso,scheduling,metastorage,router,resourcemanager}/server/grpc_service.go`
和 `pkg/mcs/utils/util.go`。PD 主节点条件注册 ResourceManager。

`go run ./scripts/rust-proto-oracle` 使用当前 Go 模块的真实 gogo 序列化器输出
`pkg/proto/wire-fixtures.json`：RequestHeader、RegionHeartbeatRequest、TsoRequest、
TsoResponse、GetMembersResponse、显式零值 oneof、错误枚举。Rust 测试验证字段语义与
逐字节重新编码；descriptor 测试检查全部服务、关键字段号和双向流方法。
这些是协议基线测试，不代表完整 Go/Rust 服务行为差分已完成。

先按任务规则领取共享槽位并设置绝对 CARGO_TARGET_DIR，再执行：

```sh
cargo test --locked -p astersql-pd-proto --lib -- --nocapture
cargo tree --locked -p astersql-pd-proto -d
./scripts/check-rust-proto-clean.sh
```

漂移脚本比较 Go fixtures、两次真实生成的 Rust 文件及 descriptor 哈希，检查协议依赖
单版本且没有 protobuf/grpcio/OpenSSL，并要求对应 tracked 文件无 unstaged diff。
新基线审查暂存后可运行脚本；脚本不自行领取或释放调用者持有的 Cargo 槽位。
