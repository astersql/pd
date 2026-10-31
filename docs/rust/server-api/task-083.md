# 任务083：接入 TLS 监听

状态：已完成。

基准 `0a242e5ae`；已核验任务82完成证据、任务1–3三份清单及任务4只读oracle入口。默认features，kvproto仍固定 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。保留已有prompt.md变更；plan.md、所有Go及Go模块文件只读。

## 契约与实现

Go的 `server/config/config.go:736–744` 将安全配置交给embedded-etcd客户端listener；`pkg/utils/grpcutil/grpcutil.go:116–134` 声明HTTP/1.1与h2协议。这里复用任务82的[认证listener](../../../pkg/server/service_auth.rs)，通过[tls_listener.rs](../../../pkg/runtime/tls_listener.rs)接到原[宿主生命周期](../../../pkg/runtime/node.rs)。HTTPS在任何Member/bootstrap/选主写入之前读取并验证证书、密钥及CA；HTTP带TLS凭据明确报错，HTTPS缺少有效凭据也报错，不降级明文。握手中的客户端证书链由现有rustls verifier验证，SAN由真实客户端验证；无匹配ALPN的握手拒绝。HTTP/1.1和tonic h2共享同一个原router和MetaStore，保留真实peer地址、ready、取消和graceful shutdown。

`PdNodeBuilder::with_listener_config`消费canonical Config，默认HTTP行为保留。Endpoint只做地址解析与TCP预留，HTTPS由宿主装配TLS；原任务39的“所有HTTPS绑定都报错”断言因此改为验证预留/释放。仍验证宿主缺少TLS凭据时启动拒绝且端口释放。CLI补 `--cert`、`--key`、`--cacert`并要求监听/advertise的scheme一致。CLI和地址校验是实际HTTPS入口的必要相邻接线，因此扩大到对应Rust源和测试；唯一Cargo.lock变化为CLI依赖已有workspace config crate，没有增加外部依赖或feature。

上述“写入之前”指PdNodeBuilder.start的TLS校验边界。独立CLI先调用既有bootstrap_config获得cluster/member身份，再进入start，故CLI错误证书仍可能已写入身份；不把嵌入start的零写入断言冒充CLI全流程零写入。出站TLS、转发凭据、证书热更新、完整配置文件/flag兼容分别留给84/85/90/91；外部etcd仍是过渡MetaStore，不宣称Go embedded-etcd拓扑等价。

## 测试与差分

[独立runtime测试](../../../pkg/runtime/tls_listener_test.rs)在lib.rs显式注册，使用真实TCP、curl HTTP/1.1、tonic h2与真实etcd MetaStore。成功HTTP状态为200；GetMembers观察HTTPS成员地址，AllocID必须返回正ID，持久化分配边界不得小于返回ID。错误服务端CA、非信任客户端证书、错误SAN和不支持ALPN分别拒绝，拒绝前后比较全部PD持久entries（含revision）不变。未完成握手时取消宿主，读取观察EOF/断开，shutdown有界完成，原端口立即重新绑定。无效TLS文件/HTTP凭据配置启动失败、释放端口且不写MetaStore。

[CLI独立测试](../../../cmd/pd-server/main_test.rs)从真实flags进入run_until，在真实etcd和原节点上执行带证书curl请求，然后关闭并重绑端口。另有显式Cargo集成target [tls_listener_test.rs](../../../cmd/pd-server/tls_listener_test.rs)，启动实际Rust pd-server二进制，带证书HTTP成功后发送SIGTERM，验证成功退出、PD stopped及端口重新绑定。未新增Go测试/生成器/模块依赖。

初始测试通过已有公共start入口请求HTTPS，实际编译运行后因 `TLS listener is not configured` 在“HTTPS宿主应可启动”的行为断言失败：退出101，1失败，0忽略。此时旧入口没有TLS配置API；该红灯证明旧宿主拒绝HTTPS，不声称当时已读取证书。实现后将测试扩展为带真实证书的builder配置及完整成功/拒绝/取消场景，没有以编译错误或人工禁用已有TLS实现作为红灯。

[只读差分supervisor](../../../scripts/tls-listener-oracle.py)复用任务82证书生成器和任务4Go只读守卫，启动已有 `bin/pd-server`，以同一组临时证书执行当前runtime测试。Go与Rust均观察同端口HTTP200与相同Ping正文、GetMembers/AllocID h2成功；错误服务端信任链、客户端链、SAN及ALPN均拒绝。ID/cluster ID只比较有效性和Rust持久边界，随机身份不作数值相等比较；Go持久分配沿用真实AllocID路径，不声称直接读取Go底层数据。ALPN失败必须实际包含 `no application protocol`，不是任意网络失败。Go二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8，build info无VCS revision，不能声称来自当前HEAD。

证书/私钥仅在槽位临时目录生成并删除，证据不包含私钥；Go进程和Cargo子进程均wait/reap，SIGTERM经finally清理。不运行make、failpoint启停、Go格式化/生成/整理或Go构建；既有etcd fixture仅走任务4审查过的只读Go构建链。

## 来源行覆盖

- [source-inventory](source-inventory.md)的任务83行及owner含83的共享行逐行保存在压缩证据source_rows；node入口、grpcutil TLSConfig与server/config是本次直接接线。共享文件中的区域查询、metrics、配置持久化和worker继续由各自owner负责，本任务不将整文件扫描行当成全行为重验。
- [wire-inventory](wire-inventory.md)无独立owner83的新RPC/HTTP方法。本次验证既有Ping、GetMembers、AllocID公共传输入口，不重复领域实现、不宣称263路由全部重新差分。
- [surface-inventory](surface-inventory.md)的C-181、security证书/CA路径叶子、AdjustCommandLineString与共同TLS入口由既有Config及新增CLI消费；全部48条含83/82–85的共享来源行原文保存。CN策略复用任务82；SSL*Bytes是出站客户端分支，peer TLS/独立MCS/metrics认证、redaction及embedded-etcd拓扑不在本项验收。Go独立ToServerTLSConfig的可选客户端证书语义不替代这里classic embedded-client listener的强制CA认证语义。

## 验证记录

动态依次原子mkdir领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；持有会话与宿主PID在自己的锁目录登记，所有Cargo/fixture子进程继承该绝对路径，未绕过target-dir。`CARGO_BUILD_JOBS=4`，测试使用`RUST_TEST_THREADS=4`。Rust修改后先运行cargo fmt --all。

| 确切命令 / 阶段 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo test --locked -p astersql-pd-runtime --lib task_083_ -- --nocapture`（初始行为红灯） | 101 | 1失败，0忽略 |
| `cargo fmt --all`（每次Rust修改后） | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_083_ -- --nocapture`（最终） | 0 | 2通过，0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture`（最终） | 0 | 59通过，36既有忽略 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --nocapture`（最终） | 0 | 5通过，0忽略（含1个实际进程集成测试） |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 2175通过，76既有忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/tls-listener-oracle.py` | 0 | 1个真实Go/Rust差分测试，0忽略 |

初次包级回归因任务39旧HTTPS拒绝断言失败（58通过、1失败、36忽略，退出101）；更新为新的HTTPS预留契约后最终包级通过。工作区回归在新增独立进程集成target之前运行；该最终target随后由CLI包级5项测试单独覆盖，生产Rust在二者之间未变更。原始失败日志均保留。


Go守卫及只读plan校验、进程/锁清理和原始日志见[压缩证据](task-083-evidence.json.gz)。编号任务在获得证据后标记完成并删除；不修改plan.md和来源史诗。

Go前后守卫1101项存在性、内容SHA-256与Git状态完全一致；plan.md SHA-256 `a59eba65612e814ada50274525cbb41a81df792f2d2966c2e60a2deae5d84615`与HEAD字节一致。自己的slot-1锁及占用记录已释放；ps复核无槽位服务/fixture残留，编译缓存保留。Python AST、文档链接、git diff --check及gzip/JSON往返通过。原始证据SHA-256 `989712867f5e7169ea319fef218ecfda6a794b7da22fdc228a63150928eabd5a`。
