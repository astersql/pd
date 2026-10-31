# 任务084：双向 TLS 宿主验收

状态：已完成。

基准 `87766a863`；依赖任务83及任务1–4证据已读取并核验。Go、Go模块与只读plan.md均未修改，保留用户已有prompt.md变更。kvproto仍固定 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认features，没有新增依赖或平行listener。

## 契约与复用

Go [GenEmbedEtcdConfig](../../../server/config/config.go#L736)在CA路径非空时强制客户端认证，并仅在CN列表长度大于1时应用允许列表；单元素列表即使不包含客户端CN，也仍接受可信客户端。这与[ToServerTLSConfig](../../../pkg/utils/grpcutil/grpcutil.go#L116)独立辅助函数的可选证书语义不同，本任务验收classic客户端入口，未改变peer/出站TLS语义。

任务82的[认证listener](../../../pkg/server/service_auth.rs)已经提供真实链、有效期及CN验证；任务83的[TLS宿主接线](../../../pkg/runtime/tls_listener.rs)已经复用它。本次通过当前公共PdNodeBuilder入口重验现有能力，只增加缺失的[独立测试](../../../pkg/runtime/mtls_listener_test.rs)及显式模块注册，将相邻TLS测试的证书/HTTP helper开放给兄弟测试复用。没有人为撤销已有能力制造失败，也无需新增mtls_listener.rs或修改生产认证逻辑。

## 真实行为与差分

`task_084_behavior`分别以多元素、单元素和空CN列表启动真实宿主，每种配置使用独立etcd MetaStore。curl HTTP/1.1得到200，tonic h2 GetMembers取得cluster身份后调用AllocID，返回正ID且Rust持久化分配边界不小于该ID。可信但不允许CN仅在多元素模式下被拒绝；其余两种配置HTTP及RPC均接受该客户端。

每种配置的过期、未知CA、自身未携带证书客户端，HTTP及gRPC均拒绝。过期证书复用允许CN的CSR和受信CA，固定有效期为2000-01-01至2000-01-02，无睡眠和时间竞态；验证使用真实TLS证书时间检查，不把时间依赖替换为mock。拒绝请求前后逐项比较PD持久entries（包括revision）不变，受限CN拒绝也单独比较。未完成TLS握手时取消，shutdown在5秒内完成，连接结束且同端口可立即重新绑定。

[只读差分supervisor](../../../scripts/mtls-listener-oracle.py)复用任务4只读守卫及任务82证书生成器，启动三个既有Go pd-server进程，以相同证书和CN配置运行同一个Rust行为测试。三组真实Go/Rust均观察HTTP200/相同ping正文、h2 AllocID成功与全部拒绝边界；随机ID只比较有效性，不作数值相等断言。Go底层存储未直接读取，不把Rust持久断言当作Go持久差分。Go二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8，无VCS revision，不能声称来自当前HEAD。

首轮新测试实际运行后失败于第二个配置的IdentityConflict（退出101、1失败）；原因是测试复用持久身份但改变监听地址，第一种配置的mTLS已成功。这是测试隔离问题，不是缺失mTLS的红灯；改用独立etcd fixture后完整通过。已有实现场景按计划直接补证据，不以编译错误或人为失败证明缺陷。

## 来源覆盖与边界

[来源清单](source-inventory.md)任务84行已更新；[协议清单](wire-inventory.md)无独立owner84的新方法；[表面清单](surface-inventory.md)的C-181、security证书/CA/CN叶子及共同TLS调用入口由当前canonical Config、宿主和真实差分覆盖。全部owner含84及范围涵盖84的来源原文保存在压缩证据source_rows（source 109、wire 0、surface 49行），包括跨任务大范围共享行；这些共享行只验收本任务的客户端mTLS部分，不宣称整文件或1–102全部行为完成。区域查询、metrics、转发、peer TLS、SSL*Bytes出站分支、热更新和完整配置文件兼容仍归各owner；外部etcd不等同Go embedded-etcd拓扑。

## 验证

从仓库根依次原子mkdir动态领取slot-1，记录任务、会话及持有PID；所有Cargo与fixture子进程继承 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`，未覆盖target-dir。Rust修改后先运行 `cargo fmt --all`（退出0）。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo test --locked -p astersql-pd-runtime --lib task_084_ -- --nocapture`（首轮/最终） | 101 / 0 | 1失败（隔离问题）/ 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 60通过，36既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 2177通过，76既有忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/mtls-listener-oracle.py` | 0 | 1行为测试，3组真实Go/Rust差分，0忽略 |

未执行make、failpoint启停、Go格式化/生成/整理；现有etcd fixture仅执行经任务4审查的 `go build -mod=readonly`链，不新增或修改Go。证书私钥只在槽位临时目录生成并随fixture清除，证据不包含私钥。完整原始日志、Go构建信息与清理守卫见[压缩证据](task-084-evidence.json.gz)。

最终Go只读守卫1101项存在性、内容SHA-256与Git状态完全一致；plan.md与HEAD字节一致（SHA-256 `a59eba65612e814ada50274525cbb41a81df792f2d2966c2e60a2deae5d84615`）。全部相关子进程已结束，自有slot-1锁已释放，编译缓存保留。Python AST、文档链接和git diff --check通过；原始证据SHA-256 `420cec0c12a763ffd0b6fe41bc70b27beea812009e55148ae508d23c02e59709`。
