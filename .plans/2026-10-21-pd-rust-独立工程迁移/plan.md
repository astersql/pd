# PD Rust 独立工程迁移计划

目标：在当前 PD 独立仓库中建立 Rust 2024 的可持续迁移架构，保留独立构建与发布能力，并让核心运行时未来可被单一 `astersql` 二进制直接链接和装配。

范围：本计划覆盖迁移方向、Rust workspace 组织、依赖基线、协议兼容测试、元数据抽象、第一条可运行 PD 主链和原生 Rust 元数据引擎的验证。Go 实现继续作为行为基准；Rust 代码与 Go 代码在本仓库并存，按模块逐步替换。

范围外：当前不把 PD 源码合并进 `astersql-tidb`，不在本阶段完成全部 scheduler、工具、Dashboard 和 Go client 的移植，也不把 TiKV 存储节点合并进一个进程。

假设：最终的“单一二进制”首先指一个 Rust `astersql` 可执行文件能够装配 SQL 与 PD 角色；PD 仍须支持独立角色部署。AsterSQL 当前 Rust 2024 workspace、`astersql-*` crate 命名、薄二进制入口和 Go/Rust 同目录并存模式是组织参考，不复制其现有桩实现。

## 设计决策

- PD 继续是独立 Git 仓库、独立 Cargo workspace、独立测试和发布单元；当前不修改 `astersql-tidb`。
- 采用 library-first：`cmd/pd-server/bin_main.rs` 只做进程级启动，实际生命周期由可复用的 `astersql-pd-runtime` 暴露，禁止核心库调用 `process::exit`、安装全局信号或私自创建多个 Tokio runtime。
- Rust 文件与 Go 文件同目录并存；一个稳定 Go package 对应一个 canonical Rust crate，名称使用 `astersql-pd-*`，根 crate `astersql-pd` 仅作为兼容门面。
- Rust 统一使用 edition 2024、workspace resolver 3、集中依赖版本和 `Cargo.lock`；生产测试放在独立 `*_test.rs` 文件，不在生产文件内堆积测试实现。
- 外部协议以 kvproto wire compatibility 为准。新服务端优先 `tonic`/`prost`，HTTP 使用 `axum`/`hyper`/`tower`，TLS 使用 `rustls`，异步运行时统一为 Tokio，日志统一为 `tracing`，指标统一为同一版本的 `prometheus`。
- 领域模型不得直接暴露 protobuf 类型；gRPC/HTTP 仅是适配层，以免协议生成方式锁死调度、存储和核心数据结构。
- 元数据采用两阶段策略：先实现 `MetaStore` 契约和 `EtcdMetaStore`，快速复现 PD 行为；再实现内嵌 Rust 共识后端。生产替代完成前不得依赖外部 etcd 才能启动。
- 不通过 Go FFI、cgo 或把 Go runtime 静态塞入 Rust 二进制解决 embedded etcd；这与统一 Rust 技术栈目标冲突。
- 最终单二进制采用“同一构建产物、可组合角色”，而不是强迫每个进程同时承担 SQL、PD 和所有后台任务。独立 PD 模式和未来嵌入模式必须共用同一运行时实现。

## 架构说明

建议的第一阶段目录形态：

```text
Cargo.toml                         # workspace + 根 facade package
Cargo.lock
rust-toolchain.toml
rustfmt.toml
pkg/lib.rs                        # astersql-pd facade
pkg/core/{Cargo.toml,lib.rs,*.rs}
pkg/storage/{Cargo.toml,lib.rs,*.rs}
pkg/member/{Cargo.toml,lib.rs,*.rs}
pkg/tso/{Cargo.toml,lib.rs,*.rs}
pkg/schedule/{Cargo.toml,lib.rs,*.rs}
pkg/server/{Cargo.toml,lib.rs,*.rs}
pkg/runtime/{Cargo.toml,lib.rs,node.rs}
pkg/proto/{Cargo.toml,build.rs,*.proto-or-generated}
cmd/pd-server/{Cargo.toml,bin_main.rs,lib.rs,main.rs}
tests/rust-compat/
docs/rust/
```

依赖方向必须保持单向：

```text
domain/core <- storage/member/tso/schedule <- server adapters <- runtime <- bin_main
      ^                                                        |
      +---------------- compatibility test harness ------------+
```

`runtime` 对未来 AsterSQL 暴露类似 `PdNodeBuilder::new(shared_runtime, config)`、`start()`、`ready()`、`shutdown()` 的生命周期接口。独立 `pd-server` 自己负责创建 runtime、解析 CLI 和处理信号；未来 `astersql` 可以传入共享 runtime、取消令牌、指标注册表和监听器。

## 依赖迁移方向

| Go 依赖/能力 | Rust 方向 | 约束 |
| --- | --- | --- |
| `kvproto`、gogo/golang protobuf | AsterSQL kvproto fork新增并发布 `prost`/`tonic` 生成能力 | 必须固定 tag/commit；先解决当前 PD 与 AsterSQL kvproto 版本漂移 |
| `google.golang.org/grpc` | `tonic` + `tower` | wire 兼容；生成类型不进入领域层 |
| Gin/Gorilla/Negroni/cmux | `axum` + `hyper` + `tower`，gRPC/HTTP 同监听器路由 | 保持现有端口与探针兼容 |
| embedded etcd server | `MetaStore` + `EtcdMetaStore`，最终为内嵌 Rust Raft 后端 | 不实现完整 etcd；只实现并验证 PD/AsterSQL 实际需要的事务、revision、watch、lease、选主语义 |
| etcd client | `etcd-client` | 仅作为过渡后端和兼容适配器 |
| LevelDB | 持久化引擎通过 trait 隔离，依据 durability 原型选择 | 不因为 `astersql-tidb` 已有某 crate 就直接决定生产引擎 |
| zap/pingcap log | `tracing` | 统一结构化字段、span 和脱敏策略 |
| pingcap/errors/errcode | 叶子 crate 用 `thiserror`，进程边界用 `anyhow`，保留稳定错误码 | 禁止用字符串比较模拟错误类型 |
| failpoint | Rust failpoint facade + 确定性故障注入接口 | 必须覆盖原有 232 个注入点的迁移清单 |
| Prometheus Go client | 单一版本 `prometheus` | 指标名、label、help、类型保持兼容，避免高基数 |
| TOML/Cobra/pflag | `serde`/`toml` + `clap` | 配置默认值、废弃字段、CLI 退出码保持兼容 |
| TLS/OpenSSL | `rustls` 为默认 | grpcio/OpenSSL 只允许留在过渡客户端边界 |
| AWS KMS/STS | AWS Rust SDK | 统一凭据链和敏感信息脱敏 |
| tidb-dashboard | 后期单独处理静态资源和 API 适配 | 不允许 Dashboard 阻塞核心 PD 可用性 |
| Go `.so` scheduler plugin | 后期改成稳定 RPC 或 WASM 扩展边界 | Rust 不承诺加载 Go plugin ABI |

## 长期迁移波次

1. **基础设施波次**：workspace、协议生成、配置、错误、日志、指标、故障注入和差分测试。
2. **控制面骨架波次**：`core`、`storage`、`member`、ID allocator、TSO、基础 gRPC/HTTP 和生命周期。
3. **集群状态波次**：Region/Store heartbeat、region tree、统计、同步和热区状态。
4. **调度波次**：operator、checker、placement rule、scheduler，再迁移 scheduling MCS；以每个 scheduler 的差分场景为任务边界。
5. **扩展服务波次**：resource manager、router、meta storage、keyspace、GC、unsafe recovery、encryption 和 metering。
6. **工具与客户端波次**：保留 Go client 的 wire compatibility；按需要提供 Rust client。`pd-ctl`、recover、backup、simulator 分开迁移，不阻塞服务端切换。
7. **原生元数据与发布波次**：Rust Raft 元数据后端、滚动升级、快照/恢复、混合版本、长期故障测试和 Go 实现退役。

每个后续波次应建立独立计划目录；本计划只执行第一阶段架构和纵向 MVP，不把整个 19 万行生产代码塞进一个任务集。

## 第一阶段成功标准

- `cargo metadata` 能解析 Rust 2024 workspace，独立 `pd-server` Rust 二进制可构建并通过启动/关闭测试。
- 协议来源固定，同一测试能分别调用 Go 与 Rust 服务并比较响应、状态码和序列化结果。
- `MetaStore` 的事务、revision、prefix watch、lease 和选主语义有契约测试，Etcd 后端通过。
- 最小 Rust PD 节点能完成 member bootstrap、ID 分配、单调 TSO、健康检查和至少一条 PD gRPC 请求。
- 核心 runtime 可以由独立二进制创建，也可以由测试进程传入共享 Tokio handle 创建，证明未来可嵌入而当前仍保持独立工程。
- 原生 Rust 元数据后端完成明确 ADR：通过故障/恢复原型后进入后续实现计划；未通过则保留可替换接口，不伪装为生产可用。

## 开发策略

- 对会改变行为的代码，使用失败验证-通过验证-重构。
- 在实施步骤前规划失败验证命令和预期失败。
- 将实现步骤限制在失败测试所证明的行为范围内。
- 测试真实行为，不验证模拟对象或实现细节。
- 每个移植任务先固定对应 Go 源文件、测试和协议观察值，再写 Rust；Go 版本持续作为 oracle，直到对应模块完成切换。
- 所有长期运行测试都要能选择 Go、Rust 或双实现；Rust 结果只有在差分通过、故障注入通过且没有桩实现时才计为完成。

## 批次

- 批次 1：任务 1。
- 批次 2：任务 2，依赖批次 1。
- 批次 3：任务 3，依赖批次 2。
- 批次 4：任务 4，依赖批次 3。
- 批次 5：任务 5，依赖批次 4。
- 批次 6：任务 6，依赖批次 5。
- 批次 7：任务 7，依赖批次 6。
- 批次 8：任务 8，依赖批次 7。
- 批次 9：任务 9，依赖批次 8。

