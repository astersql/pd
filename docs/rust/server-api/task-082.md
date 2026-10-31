# 任务082：客户端证书认证与访问边界

状态：已完成。

基准 `d81f68198`；已读取只读 plan.md、任务82、任务1–4的准入/清单与任务81完成证据。保留原有 prompt.md 修改。Go、Go模块文件及plan.md只读；固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认features。

## 契约与实现

Go的classic PD客户端入口由 `server/config/config.go:736–744` 的 `ClientTLSInfo` 配置，经既有etcd embed的客户端TLS listener执行认证。配置CA路径时必须验证客户端证书链；CN列表超过一项时才检查CN，零项/单项保留历史兼容。没有CA时不要求客户端证书。`Authorization`、Caller-ID、转发证书header不是该入口的认证协议，也没有凭空增加角色/RBAC协议。

[service_auth.rs](../../../pkg/server/service_auth.rs)提供宿主拥有的 `Listener::new(tcp, &Config)`，直接消费已有 `security.tls` 的CA/cert/key路径及CN列表，不维护第二套安全配置。WebPkiClientVerifier先验证链、用途、有效期及签名，CN检查只在链验证成功后执行，并继续委托TLS1.2/1.3握手签名验证。采用Go最后一个subject CN的语义，支持普通字符串、T61和BMP编码；无法解析的CN拒绝，不能误当空CN放行。没有自定义证书信任旁路或读取请求header建立身份。

同一认证listener上的HTTP/1与HTTP/2 tonic都在进入原[router](../../../pkg/server/lib.rs)、限流、审计、请求角色、领域handler或转发之前完成认证；拒绝发生在TLS层，没有HTTP状态/响应体或应用gRPC PermissionDenied。测试中HTTP `000` 表示curl未得到HTTP响应，RPC为连接/传输错误。通过 `PeerAddr` 将真实连接地址交给原[request_context](../../../pkg/server/request_context.rs)，不信任header替代socket地址。

握手并发有界（128），每个握手10秒超时，未发送ClientHello的连接不阻塞其它合法客户端。JoinSet由listener拥有，listener销毁时中止待完成握手；没有安装全局crypto provider、信号、日志订阅器或创建私有runtime。证书/密钥/CA读取或解析错误直接返回并释放传入socket，不降级到明文。

本任务为配置驱动的认证边界提供可用于原路由的真实TLS listener；CLI/宿主HTTPS选择及完整listener装配由任务83消费此API，出站TLS/转发凭据与证书热更新仍归任务84/85。现有PdNodeBuilder明文入口不因此宣称支持HTTPS配置。Go `TLSConfig.ToServerTLSConfig` 的独立可选客户端证书语义不是这里的embedded client listener语义；peer SAN、peer CN、metrics推送Basic/Bearer凭据、redaction以及embedded-etcd拓扑不混为PD客户端权限。

## 测试与真实差分

[独立测试](../../../pkg/server/service_auth_test.rs)由lib.rs显式注册。测试使用真实TCP、curl、tonic、原router、真实etcd MetaStore，以及canonical Member/Leadership/Allocator，未用固定ID或调用次数替代领域写入证据。每个合法AllocID验证持久分配边界不小于返回ID；HTTP配置写入从真实 `service_middleware` key恢复检查。拒绝请求前后比较全部MetaStore entries（含revision），覆盖直接HTTP读、HTTP写及RPC。另用共享MetaStore上的真实leader与认证follower检验HTTP/RPC转发：缺证书、不受信证书与错误CN均不产生持久变化，允许CN完成转发写入。

四种配置 × 四类客户端共16组准入场景：有CA+多CN、有CA+单CN、有CA+空CN、无客户端认证；客户端分别缺证书、自签非信任证书、可信CA但CN不允许、可信且CN允许。伪造Bearer与客户端证书header不能绕过认证。未完成握手时宿主取消，已连接的客户端观察EOF；服务任务完成，原端口可立即重新绑定。错误密钥文件同样验证listener释放。

初始接口已能编译并执行，尚未接入CN白名单时，`denied`可信证书获得HTTP200，测试在“应拒绝”的行为断言失败，退出101，1行为失败。此前一次Connected实现遇到Rust orphan编译错误，修正为本地PeerAddr后才取得上述行为红灯，编译错误不计TDD证据。开发中另发现并发证书fixture目录生命周期冲突，现用PID+原子序号隔离；原始失败日志保留。RPC副作用验证进一步改用canonical allocator后重新验证。

[只读Go supervisor](../../../scripts/service-auth-oracle.py)仅启动已有bin/pd-server并调用Cargo/Rust测试；不构建Go oracle，不启停failpoint、不生成或格式化Go。既有etcd fixture仅复用任务4审查过的只读Go构建链。临时CA、密钥、证书与数据目录限定在领取槽位内并在退出时删除，证据不保存私钥。

使用同一批临时证书在三个真实Go HTTPS节点（多CN、单CN、空CN）与Rust执行相同HTTP读写和tonic GetMembers/AllocID。无认证默认项比较Go原明文入口；Rust另覆盖TLS但不要求客户端证书，因此不把这一项说成Go无CA HTTPS启动差分。比较准入、HTTP状态、RPC成功/错误及未授权Go请求前后实际配置不变；ID与cluster ID只检查有效性/持久边界，不比较随机身份或不同allocator的具体ID。

首次Go启动使用单CN `not-allowed`时，其出站ToClientTLSConfig会校验服务端CN，导致Go自连接失败；改用单CN `server`允许自连接，再用CN=`denied`的客户端确认入站单CN例外。该失败是独立入站/出站语义的证据，未伪称入站单CN会强制匹配。

Go oracle SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8；构建信息没有VCS revision，不能声称来自当前HEAD。现有真实二进制、build info、配置、响应观察、原始运行日志及清理结果保存在压缩证据中。

## 来源覆盖

- [source-inventory](source-inventory.md)：任务82行及apiutil/serverapi/multiservicesapi/grpcutil/server.go共享owner行，逐行引用及分类在原始证据 `source_rows`。本任务验证这些调用链共同的客户端认证前置；Core/APIServiceGroup/RuntimeServices是服务开放条件，Caller-ID是调用来源，均不是用户凭证。共享worker、bootstrap、持久化、failpoint、metrics及出站连接行为沿用相应owner，不声称逐个重跑Go failpoint测试。
- [wire-inventory](wire-inventory.md)：没有独立owner=82的方法或新鉴权RPC；本任务覆盖原HTTP和pdpb路由的共同传输前置，采用真实Ping、配置管理、GetMembers、AllocID与转发路径观察，不重复实现领域handler或宣称全部263路由领域行为重验。
- [surface-inventory](surface-inventory.md)：C-181、security路径/CN冻结叶子与grpcutil/server.GetTLSConfig共享行由已有Config直接接入。SSL*Bytes是Go出站客户端分支，客户端嵌入listener仍按文件配置；CLI开关装配、出站TLS、证书轮换及metrics推送认证各保留83/84/85/86的owner。原始证据保留全部共享82行，区分认证适用部分和其他owner，不把共享源码扫描行误算成新的权限能力。

## 验证记录

动态依次原子mkdir领取共享槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`。所有Cargo及fixture子进程继承该路径，没有--target-dir覆盖。Rust修改后先运行cargo fmt --all。

| 确切命令 / 阶段 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo test --locked -p astersql-pd-server --lib task_082_ -- --nocapture`（未接入CN校验） | 101 | 1行为失败、0忽略 |
| `cargo fmt --all`（最终Rust修改后） | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_082_ -- --nocapture`（最终） | 0 | 2通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture`（最终） | 0 | 271通过、39既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 2172通过、76既有忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/service-auth-oracle.py`（同一槽位） | 0 | 1项真实Go/Rust差分测试、16组准入矩阵，0忽略 |

Go前后守卫1101项文件（含ignored/untracked）的存在性、内容SHA-256及Git状态完全一致；plan.md与基准字节一致，SHA-256 `a59eba65612e814ada50274525cbb41a81df792f2d2966c2e60a2deae5d84615`。没有Go或模块文件变更。实际Go/Rust服务、fixture与Cargo进程已结束；ps复核本槽位无活跃服务/fixture进程。自己的slot-1占用记录及锁目录已释放，编译缓存保留。Python AST、文档链接、gzip/JSON往返及git diff --check通过。保留原有prompt.md变更。

[压缩原始证据](task-082-evidence.json.gz)，SHA-256 `c7a12da9002af7679ebb5a4d140d389fa188c0bc993a8af34042bab3542dd2b7`，包含行为红灯、开发故障、最终聚焦/包级/workspace完整日志、逐场景真实Go差分、输入hash、共享来源行引用与Go前后守卫。编号任务标记完成后按要求删除；不修改plan.md或来源史诗。
