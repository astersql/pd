# ADR-001：PD Rust 独立工程与运行时边界

状态：架构决策已接受；实现与验证尚未开始。日期：2026-10-21。
本文是仓库边界、依赖方向和禁止路线的权威说明；依赖选择见
[依赖基线](dependencies.md)，实施要求见[移植指南](porting-guide.md)。

## 背景与范围

当前 Go 服务在 [server/server.go](../../server/server.go) 调用
`embed.StartEtcd`，生命周期依赖 Go embedded etcd。Go 根模块和
[client 子模块](../../client/go.mod) 继续作为行为基准和兼容客户端。
本次只固定架构，不创建 workspace，不宣称 Rust 服务已经可以运行。

当前不合仓：PD 保持独立 Git 仓库、独立 Cargo workspace、测试与发布单元；
当前不修改 astersql-tidb。其 Rust 2024、薄入口和 crate 命名仅作为组织参考，
不复制桩实现。未来集成由消费方链接公开库完成，不要求迁移源码或 Go runtime。
第一阶段限于基础设施与纵向 MVP；完整调度、工具、Dashboard、Go client 移植
和 TiKV 节点进程合并均不在此阶段范围内。

## 决策：library-first

使用 Rust 2024、workspace resolver 3、集中依赖和提交到仓库的 Cargo.lock。
Rust 与 Go 文件同目录并存，一个稳定 Go package 对应一个 canonical Rust crate，
名称为 `astersql-pd-*`；根 `astersql-pd` 只重导出兼容 API，不重复实现。
以下是后续任务拟创建的布局，不代表这些文件现在存在：

```text
Cargo.toml / Cargo.lock / rust-toolchain.toml / rustfmt.toml
pkg/lib.rs                                  # 根 facade
pkg/{core,storage,member,tso,schedule}/       # 各自 Cargo.toml、lib.rs
pkg/{proto,server,runtime}/                  # 协议、适配、生命周期
cmd/pd-server/{Cargo.toml,bin_main.rs}        # 独立进程入口
tests/rust-compat/                           # 差分测试
```

依赖箭头表示“被左侧依赖”：

```text
core <- storage/member/tso/schedule <- server adapters <- runtime <- bin_main
proto <------------------------------ server adapters
```

server adapters 依赖 proto，负责协议生成类型与领域类型之间的转换。
core 不依赖 proto、网络框架或具体存储引擎；storage 的公开契约使用领域类型。
member、tso、schedule 通过契约使用存储，禁止反向依赖 server/runtime。
测试 harness 可以装配各层，不得成为生产库依赖。proto 是协议边界 crate。

`astersql-pd-runtime` 提供拟议接口 `PdNodeBuilder`、`start()`、`ready()`、
`shutdown()`；具体签名由运行时任务确定。宿主传入 Tokio handle、取消令牌、
指标 Registry 和可选监听器。start 失败须回收已取得的资源；ready 只有在元数据、
角色初始化和监听器就绪后成功；shutdown 取消并等待后台任务，释放监听器和 lease，
重复关闭应安全。库不得调用 `process::exit`、安装全局信号处理器、全局日志订阅器
或私自创建 Tokio runtime。独立二进制负责 CLI、runtime、日志初始化、信号和退出码。

未来单一 astersql 构建产物可装配 SQL 与 PD 角色，不要求每个进程开启全部角色。
独立 PD 和嵌入模式必须共用运行时；后续需以共享 handle 的宿主测试证明可嵌入性。

## 决策：协议与兼容边界

以 kvproto wire compatibility 为准，采用 tonic/prost、axum/hyper/tower、rustls、
Tokio、tracing 和单一版本 prometheus；版本锁定责任见依赖基线。
gRPC/HTTP 适配层负责生成类型转换、认证和参数校验，领域 API 不暴露 protobuf 类型。
同监听器路由须验证现有端口、HTTP 探针和 gRPC 客户端兼容，不能仅凭框架能力认定完成。

Go 是 oracle：比较 gRPC 方法、字段编号、oneof/默认值、流行为、状态码、错误码，
以及 HTTP 路径、JSON、状态码、配置默认值/废弃字段、CLI 退出码与 Prometheus
指标名、label、help、类型。ID 唯一性、TSO 单调性、选主和故障恢复必须检查语义。
非确定字段仅按明确规则归一化，不可忽略错误差异或只比较请求成功。

## 决策：MetaStore 两阶段路线

先定义后端无关 MetaStore 契约，再实现 EtcdMetaStore 作为过渡与 oracle。
契约必须定义事务 compare/原子提交、revision 分配与一致读、prefix watch 的顺序/
起始 revision/压缩后错误、lease TTL/续期/到期删除，以及 lease 绑定选主与 fencing。
同一套契约测试运行于 Etcd 和未来原生后端，不能以 CRUD 成功替代语义验证。

第二阶段评估内嵌 Rust Raft 元数据后端，只覆盖 PD/AsterSQL 需要的语义，
不承诺完整 etcd API。持久化引擎保持 trait 隔离；在 durability 原型验证掉电、
日志/快照恢复、崩溃一致性和故障选主前，不选定生产本地引擎。
Etcd 过渡模式可以要求外部服务，但生产替代验收必须证明无外部 etcd 也能启动；
在此之前不能宣称完成 Go embedded etcd 的生产替代。

## 禁止路线与验收

禁止 Go FFI、cgo 或静态嵌入 Go runtime 来保留 embedded etcd；禁止把 grpcio/
OpenSSL 扩散到新服务端、把 protobuf 泄漏到领域模型、重复实现 facade 或将桩标为完成。
Go scheduler plugin ABI 不承诺兼容，后期另评估 RPC/WASM 扩展。

后续验收须获得独立构建/启动/关闭、Go/Rust 协议差分、MetaStore 契约、member
bootstrap、ID、TSO、健康探针、至少一条 PD gRPC 请求，以及共享 runtime 装配证据。
原生后端需单独 ADR 记录故障恢复原型结果；未通过时保留可替换接口。
