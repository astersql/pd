# 任务042：gRPC领导者转发

状态：已完成（经典PD明文单跳gRPC转发边界）。

基准 `0d6ac2004`，依赖任务041的实现与验收记录已核验。Go与模块文件只读，plan.md只读；固定kvproto commit仍为 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 实现边界

[grpc_forward.rs](../../../pkg/server/grpc_forward.rs)在[统一router](../../../pkg/server/lib.rs)接线，复用已有MetaStore发现与请求取消上下文。按Go `unaryFollowerMiddleware`、`validatePDForwardedHost`及`ResetForwardContext`的契约，非空`pd-forwarded-host`先与当前leader地址匹配，允许HTTP(S) scheme差异，转发前清空该metadata。无metadata时保持原本地角色检查；GetMembers、GetClusterInfo、QueryRegion、SyncRegions、global config及废弃/不支持方法不进入转发。

白名单共44个方法，静态逐项比较与Go调用者集合相同（不计行为测试）。覆盖经典PD的Go unaryMiddleware/unaryFollowerMiddleware调用者（含gc_service.go），以及Tso、RegionHeartbeat、ReportBuckets流。协议仍由现有生成服务和领域适配处理；传输代理不重新实现领域行为。GC领域自身仍由78及主题8验收，转发成功不等于本地领域已实现。

使用真实HTTP/2逐帧传递body和trailers，保留重复ASCII、二进制metadata及上游状态。连接future由请求/响应body持有，不创建生产后台代理worker。保留原截止时刻，扣除发现与拨号耗时后编码上游grpc-timeout；本地同时执行原deadline。客户端取消、响应drop和宿主取消均终止上游请求。请求只发送一次；即使持久写入后返回错误或leader改变，也不重试、不回放。

当前宿主为明文传输，沿用Go“URL scheme与配置的传输独立”的规则；TLS材料和加密传输由83–85负责。本任务没有新增TLS、连接池、MCS代理、指标或领域实现，不引入新Cargo依赖。已有Go的TSO代理专用内部超时/批处理以及MCS重连策略不在当前经典单跳传输验收内。

## 来源清单映射

- [source-inventory](source-inventory.md)80行的任务42：由本实现和以下真实行为/差分验证覆盖。
- 205–208、410–411、2518–2525行grpcutil共同owner：本任务消费ForwardMetadataKey、ResetForwardContext、GetForwardedHost及地址/连接契约；cluster.go领域调用仍复用已有各RPC适配；TLS和RequestCounter分别保留83–85与后续观测owner。未移植Go工具函数副本。
- 317、674–713、2704行grpc_service.go共同owner：本任务覆盖转发相关unary/stream发送接收、取消与customTimeout对应的deadline边界；各RPC领域、MCS和global-config原有owner不变。发送/取消由Rust网络故障测试证明，没有运行或改写Go failpoint。
- 额外只读检查server/forward.go的目标校验及scheme规则、server/gc_service.go的unaryMiddleware调用者，避免遗漏共享转发入口。
- [wire-inventory](wire-inventory.md)没有独立42 owner行；本次给允许的方法提供共享转发传输，单方法领域证据仍由原owner提供。
- [surface-inventory](surface-inventory.md)没有独立42 owner行；本次不新增配置或指标。

## 行为与差分

[独立测试](../../../pkg/server/grpc_forward_test.rs)在lib.rs显式注册。

- `task_042_behavior`：真实两个PdNode共享真实etcd MetaStore。实现前请求已编译运行，在follower的AllocID返回Unavailable/not leader，断言失败（0通过/1失败，退出101）。实现后分配成功；同一双向Tso流发送两批请求，返回count=1/3及递增timestamp。节点shutdown/join。
- `task_042_metadata_cancel_errors_and_no_replay`：真实HTTP/2上游与Rust代理共享真实etcd；请求字节在事务中写入后读回，重复/二进制metadata与清空转发头验证；上游Aborted与响应metadata、数据后错误trailers保留。故障请求在事务提交后等待，变更leader记录再放行错误，最终事务entry/revision完全相同，证明未重放。还验证无leader的Unavailable、旧目标拒绝、unary截止时间、headers前取消、body首帧后取消、宿主取消和GetMembers本地处理。所有服务有界join。上游是传输观察fixture，不冒充PD领域。
- `task_042_readonly_go_differential`：默认忽略，由[只读脚本](../../../scripts/grpc-forward-oracle.py)显式执行。启动三个真实Go节点，Rust代理指向同一个Go leader；Go follower和Rust follower比较AllocID递增、IsBootstrapped、错误cluster的FailedPrecondition消息、过期host的InvalidArgument消息、双向Tso两批响应。通过Rust转发Bootstrap后，在真实Go leader读回bootstrapped及store 10，确认持久副作用。Rust发现记录由独立真实etcd提供；没有把Go embedded-etcd与Rust外部MetaStore布局宣称等价。

已有Go二进制没有VCS revision，只固定SHA-256与build info，不声称来自当前HEAD。Go内部取消token不可观察，取消证据来自Rust真实网络测试，不冒充跨语言token差分。没有修改Go文件、运行Go测试或启停failpoint。

相邻任务040原测试将任意`untrusted-target`作为转发头并要求本地执行。启用目标校验后包级首次运行158通过/1失败/4忽略；已将该测试fixture补齐真实etcd leader记录及discovery_store，使用被验证的本地leader URL，仍验证metadata进入请求上下文。没有在生产代码绕过目标校验。

补充检查Go `ErrNotLeader`定义时发现无leader分支应为Unavailable；新增断言先取得Unknown与Unavailable不一致的行为失败（退出101、0通过/1失败），修正后重新完成聚焦、包级与工作区验证。

## 验证记录

所有Cargo阶段动态原子领取共享槽位：初始及最终回归使用槽位1；补充无leader失败/通过测试阶段使用槽位2。实际绝对路径为：

- `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`
- `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`

锁内记录计划、任务、会话与持有进程PID；Cargo及oracle子进程继承绝对路径。所有阶段结束后释放自己的锁，缓存保留。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-server --lib task_042_ -- --nocapture`（初始失败） | 101 | 0通过、1行为失败 |
| `cargo test --locked -p astersql-pd-server --lib task_042_metadata -- --nocapture`（无leader补充失败） | 101 | 0通过、1行为失败 |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_042_ -- --nocapture`（最终） | 0 | 2通过、1默认忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/grpc-forward-oracle.py`（最终） | 0 | 显式Go差分1通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture`（最终） | 0 | 160通过、4默认忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked`（最终） | 0 | 2041通过、0失败、37默认忽略 |

此前串行工作区命令`cargo test --workspace --locked -- --test-threads=1`也通过2041项；无leader修正后以上最终命令重新验证当前源码。工作区计数含2项compile-fail doctest；零测试不计行为验收。默认忽略的差分由上面的脚本显式运行，其他既有忽略项不冒充通过。已有deprecated等警告未关闭。

1101个Go/模块文件（含ignored/untracked）前后存在性及SHA-256完全相同，Go/模块Git状态为空；plan.md与基准逐字节相同。未运行Go测试、make、failpoint或任何Go写入工具；真实etcd fixture复用任务004的只读构建入口。Python语法、44方法白名单源码集合、文档链接及git diff --check检查通过。测试/oracle进程均已结束，自己的锁已清理。

[压缩证据](task-042-evidence.json.gz)包含原始命令与日志、失败/通过计数、Go只读快照、最终源码哈希、Go二进制build info及SHA-256、真实Go响应与进程清理记录。文件SHA-256：`666df891d7aff335d9b2b340e073ba47ef31f9f4a65728fbc7e1d2acfbcd4a1c`。
