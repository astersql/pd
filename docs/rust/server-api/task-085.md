# 任务085：TLS 材料更新

状态：已完成。

基准 `3a3c63afd`；依赖任务84与任务1–4的准入证据已核验。仅扩展现有canonical认证listener，无新依赖、feature或平行服务；plan.md只读，保留用户已有prompt.md变更。

## Go 契约与实现

`server/config/config.go:736` 将classic客户端证书、私钥及CA路径交给etcd TLSInfo；`pkg/utils/grpcutil/grpcutil.go:116`同样复用TLSInfo.ServerConfig，但其可选客户端证书策略不替代classic入口的强制mTLS。固定Go模块 `go.etcd.io/etcd/client/pkg/v3@v3.5.15/transport/listener.go` 的baseConfig启动时预校验证书对，GetCertificate在每次新握手读取证书/私钥；读取或解析错误直接拒绝当前握手。CA池由ServerConfig初始化，未提供动态CA、信号刷新或缓存最后有效证书的语义。坏材料不会中断已经建立的连接，修复文件后下一次握手恢复。

现有 `pkg/server/service_auth.rs` 原先固定启动时证书，新测试可编译并实际失败于“new handshake must reload certificate”（退出101、1失败）。最小修复在原listener安装rustls证书resolver：保持启动预校验，逐次完整握手读取并校验证书链与私钥，失败返回无证书而拒绝握手；CA和CN策略仍为启动配置。无需新建runtime/tls_reload.rs或添加更新API。与Go相同，文件对非原子替换期间的新连接可失败；调用方应先写好材料再替换。

## 真实验证边界

独立 `pkg/runtime/tls_reload_test.rs` 通过PdNodeBuilder、真实TCP和etcd MetaStore启动宿主。`scripts/tls-reload-probe.py` 用Python标准TLS客户端建立持久HTTP/1.1连接，每个新连接使用全新SSLContext，避免会话复用掩盖证书变化。访问IP不发送DNS SNI，覆盖Go注释中特别要求的无SNI重载。

依次观察：同CA签发的另一有效证书/私钥对在新连接可见；旧连接证书和200响应保持；磁盘CA替换后启动时受信客户端仍能连接；坏PEM和有效证书/不匹配私钥均拒绝新握手，旧连接继续200；恢复原材料后新连接呈现原证书并200。finally恢复所有材料、关闭连接，临时私钥不入证据或Git。

Rust在探针前后以tonic h2执行GetMembers/AllocID，ID递增；探针期间allocator持久entries及revision不变。没有对全部PD entries做错误的静态假设：首轮修复后TLS边界均通过，但全库快照因后台scheduler初始化写入而失败（退出101、1失败）；最终限定为预先通过真实RPC初始化的allocator持久边界。取消未完成TLS握手后，shutdown五秒内完成，连接结束且端口可重新绑定。

`scripts/tls-reload-oracle.py`复用任务4只读守卫与已有Go二进制supervisor，在相同文件上顺序运行完全相同的探针，Go/Rust各一次均通过全部阶段。Go HTTP ping正文在每个实现内部保持一致；不比较TLS实现特定告警文本或随机证书字节。gRPC及allocator持久断言由Rust宿主执行，未宣称Go底层持久差分。Go二进制SHA-256为 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8，无VCS revision，不能声称来自当前HEAD。外部etcd仍不等同embedded-etcd拓扑。

## 来源与验证记录

[source-inventory.md](source-inventory.md)任务85行已更新；wire清单没有独立owner85协议方法；surface清单的共同TLS材料入口及security路径只覆盖本次证书/密钥生命周期，不宣称peer/出站TLS、SSL*Bytes、配置路径热更新或共享大范围owner行全部完成。相关来源原文与运行输出保存在 [task-085-evidence.json.gz](task-085-evidence.json.gz)。

所有Cargo阶段均从根目录原子mkdir动态取得slot-1，记录任务、会话及PID；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`，默认features。每阶段shell退出释放自有锁，编译缓存保留；未运行make、failpoint启停或任何Go写入命令。fixture仅使用任务4已审查的`go build -mod=readonly`调用链。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo test --locked -p astersql-pd-runtime --lib task_085_ -- --nocapture`（原实现/最终） | 101 / 0 | 1失败 / 1通过、0忽略 |
| `cargo fmt --all` | 0 | 静态 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/tls-reload-oracle.py` | 0 | 1行为测试、Go/Rust两次完整探针、0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 61通过、36既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 2178通过、76既有忽略 |

Python AST、文档链接与`git diff --check`通过。最终Go只读守卫1101项路径、内容SHA-256与Git状态完全一致；plan.md与HEAD逐字节一致，SHA-256 `a59eba65612e814ada50274525cbb41a81df792f2d2966c2e60a2deae5d84615`。来源原文记录source 113、wire 0、surface 15行（包括范围owner共享行），仅验收本任务材料生命周期部分。

Go/Rust差分进程均已回收，临时证书目录已删除；最终ps核对无本槽位或Go oracle存活进程，自有slot-1锁已释放，缓存保留。原始失败/通过日志、Go构建信息、前后完整只读守卫、命令与退出码均保存在压缩证据。
