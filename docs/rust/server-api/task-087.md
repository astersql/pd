# 任务087：同端口协议装配

基准 `e807964a4`；2026-11-01。依赖[任务086](task-086.md)及任务1–4已核验。固定kvproto仍为 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。完整命令输出、Go只读快照、清单owner87原始行、双实现记录及清理证据见[压缩证据](task-087-evidence.json.gz)。

## 行为与实现

复用既有预绑定listener、[TLS listener](../../../pkg/runtime/tls_listener.rs)、ALPN与认证，不创建平行服务；必要修复位于[canonical router](../../../pkg/server/lib.rs)。任务拟建的shared_listener.rs没有新增生产能力需求，因此不另建包装层。HTTP和tonic路由分开，按HTTP版本、content-type和Go注册的`/pd/` HTTP前缀优先级分流。显式HTTP fallback避免后续merge丢失fallback middleware；metrics仍保留既有audit/admission作用域，使用相同协议分流。

普通未知HTTP路径为404；HTTP/2未知gRPC方法及服务为HTTP200/gRPC12；`application/grpc-web`在TLS兼容路径返回普通文本415及nosniff，不能当作合法grpc subtype。`application/grpc+proto`与`;`参数形式可进入gRPC。HTTP/1携带grpc content-type仍是HTTP；`/pd/` HTTP挂载优先于底层gRPC fallback，所以向ping POST grpc content-type返回HTTP405，不能将Go的路径优先级错误改成全局content-type优先。

workspace统一新增已在锁文件中的tower 0.5.3直接依赖，用ServiceExt调用现有tonic router，未引入新的协议版本、服务可选feature或领域实现。

## 真实验证边界

[独立测试](../../../pkg/runtime/shared_listener_test.rs)通过PdNodeBuilder和真实etcd MetaStore，在同一预绑定地址并发运行HTTP/1 ping、HTTP/2 ping及tonic GetMembers/AllocID；TLS客户端实际协商h2或http/1.1。检查RPC分配ID的持久化上界；等待真实scheduler启动事务稳定后，完整`/pd/`快照验证错误请求无持久状态副作用。取消后检查停滞TCP连接关闭、shutdown完成及原端口可重新绑定。复用既有证书fixture与只读etcd fixture；不使用mock调用次数。

[只读supervisor](../../../scripts/shared-listener-oracle.py)启动既有Go二进制的明文与TLS实例，同一测试向Go/Rust发出相同请求并逐项断言HTTP/gRPC状态及HTTP版本。Go二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；构建信息为go1.26.8/without_dashboard/darwin-amd64，无VCS revision，不宣称来自当前HEAD。Go嵌入式etcd和Rust外部MetaStore的拓扑不作为等价对象。

明确差异：Go明文cmux将全部HTTP/2连接交给gRPC，因此普通HTTP/2 ping及非grpc content-type返回415，grpc content-type的ping返回gRPC12；Go TLS注册HTTP前缀优先，支持HTTP/2 HTTP。Rust按本任务要求在明文和TLS均支持HTTP/2 HTTP，统一采用已验证的TLS分流语义。测试显式核对Go明文差异，未把它归一化为相同响应。未知gRPC错误比较HTTP状态和gRPC code，不宣称tonic与grpc-go的诊断message文本相同。

## 来源覆盖

[source-inventory.md](source-inventory.md)的任务87映射、node/lib/http公开接口与manifest，以及node持久化/worker和HTTP health/member注册行，共13条owner87来源行已原样归档。node启动/worker/持久化由真实启动、RPC、MetaStore及取消验证；lib/http由真实探针与协议边界验证。wire-inventory和surface-inventory没有单独owner87行，不将其他owner的API/指标覆盖算入本任务。Go参考包括server/server.go的UserHandlers/ServiceRegister，以及固定etcd 3.5.15 embed/serve.go的cmux.HTTP2与grpcHandlerFunc；本次运行结果而非源码推导决定上述差异。

## 验证记录

首次编译发现GetMembers请求类型不明确，修正后取得真实行为失败：未知HTTP/2路径收到HTTP200/application-grpc/grpc-status12，预期HTTP404，退出101，1项失败；不以编译错误计TDD证据。后续真实对照固定TLS错误响应与HTTP前缀优先级；一次快照断言捕获scheduler启动默认配置写入，加入有界稳定等待后仍逐条比较完整状态，没有过滤领域键或放宽错误断言。

所有Cargo及子进程动态原子领取slot-1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。默认features；回归设置`CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4`。实际确切命令、退出码、数量见下方最终补记及压缩证据。Cargo fmt在代码变更后先执行；最初依赖接线阶段的`cargo check -p astersql-pd-runtime`负责更新现有tower依赖对应的锁文件条目，最终验证均为locked。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-runtime --lib task_087_ -- --nocapture`（最初行为失败） | 101 | 1失败 |
| 同一聚焦命令（最终） | 0 | 1通过，0忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/shared-listener-oracle.py` | 0 | 1项测试，明文/TLS两组Go与两组Rust；0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 64通过，36项既有忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 281通过，39项既有忽略 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据检查 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 编译检查 |
| `cargo check --workspace --locked` | 0 | 编译检查 |

workspace首轮（`RUST_TEST_THREADS=4`）退出101：既有`http_service_limit_test::task_079_wire_and_cancellation`在shutdown后重新绑定端口时报`AddrInUse`；该测试在此前server包级回归中通过。没有修改其断言或生产生命周期；以`RUST_TEST_THREADS=1 cargo test --locked -p astersql-pd-server --lib task_079_wire_and_cancellation -- --nocapture`单独重跑，退出0，1通过、0忽略。随后以相同低并发执行完整workspace重跑，记录保留首轮失败而不覆盖原始日志。

最终`RUST_TEST_THREADS=1 cargo test --workspace --locked`退出0：2192通过，76项既有忽略，0失败。

最终Go只读守卫：1101个文件的存在性、SHA-256及Git状态前后完全一致（含untracked/ignored）；plan.md原字节不变。没有运行Go写入、make或failpoint命令。所有槽位产物子进程结束，自有owner记录及slot-1锁已删除，编译缓存保留。用户原有prompt.md差异保留，不纳入本任务提交。
