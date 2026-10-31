# 任务081：服务访问审计边界

状态：已完成。

基准 `2e60ee9b9`；批次80已完成，已核验task-001的主题6准入、任务1–3清单及task-004只读oracle入口。Go、模块文件与plan.md只读；保留原有prompt.md变更。固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认features，没有依赖变化。

[service_audit.rs](../../../pkg/server/service_audit.rs)提供宿主拥有的Sink、可控单调Clock及Record。[router_with_audit](../../../pkg/server/lib.rs)在原HTTP/gRPC listener接入可选Audit，原router保持无sink入口。宿主负责提供有界、非阻塞sink；没有安装全局日志、collector、worker或runtime。复用[ServiceLimit](../../../pkg/server/http_service_limit.rs)中的同一配置锁、codec与StorageEndpoint，读取默认开启的audit开关及即时更新；重新装配路由从真实MetaStore恢复开关，不维护第二套审计配置。

路由策略保留Go显式setAuditBackend的local-log/prometheus组合，164个唯一标签（含条件注册的FailPoint；本任务不启用或实现failpoint入口）。现有service_label解析方法与模板，未知路由和gRPC不适用这套HTTP策略。独立测试直接把只读Go router.go当作数据核对150余个单行注册，另核对多行恢复路由；没有执行Go源码。处理结果包含HTTP状态和耗时；future被丢弃时status为None。完成guard只提交一次，sink返回错误不会改变HTTP响应或已完成的持久写入。

脱敏采用数据最小化：Record只有静态路由标签、后端策略、状态与耗时；不接收路径参数、query、请求/响应body、Authorization、Cookie、Caller-ID或其他请求header。无需读取、复制、解析body后再尝试删敏感字段；畸形输入和凭证也不会经错误文本泄露给sink。结果与高精度单调耗时是本任务要求的边界记录；Go本地日志在handler前执行、指标在handler后以Unix秒计时，Rust宿主收到handler结果时再记录。本任务只保留路由选择及开关语义，不声称实现主题9日志格式、collector exposition或运维后端。

## 行为与差分证据

[独立测试](../../../pkg/server/service_audit_test.rs)由lib.rs显式注册。最小接口未启用hook时测试已编译并运行：真实ping返回200，sink记录为[]，预期一条Ping/200/7ms，退出101（1通过、1行为失败）。首次编译中错误使用Read::key/快照字段及trait成员，修正后才取得上述红灯，编译错误不算失败证据。后续Go注册清单测试暴露GetVersion/GetPDStatus遗漏，已补齐；曾有oracle JSON整数/浮点类型比较失败，修正计数解析后重跑，保留原始日志。

真实TCP/HTTP与真实etcd MetaStore验证：成功及畸形JSON错误、local-log/metrics策略、确定性7ms耗时、sink故障下成功关闭审计并持久化、禁用期间请求与重新启用请求本身不记审计、重新装配路由恢复已保存的false、重新开启后恢复记录、未知路由及真实tonic GetMembers不进入HTTP审计。未完成HTTP body的TCP断开产生一次错误记录；真实pending MetaStore watch在宿主取消后返回500并产生一次记录。记录内容逐项拒绝测试凭证与路径值；不以mock调用次数替代响应、Record及持久状态断言。并发请求可推进共享测试clock，故重叠请求耗时不强制7ms。

[只读Go supervisor](../../../scripts/service-audit-oracle.py)只启动现有bin/pd-server，复用task-004已审查fixture链，不运行make、failpoint启停或Go生成/格式化。相同注册请求序列在真实Go与Rust执行：200 ping、400畸形配置、200关闭审计、禁用期间ping、200重新开启、200 ping。Go真实metrics增量逐步为(Ping,SetServiceMiddlewareConfig)=(1,0),(1,1),(1,2),(1,2),(1,2),(2,2)，与Rust实际Record一致；Go实际local-log两条，对应Rust同两条local_log策略记录。两边断开未完成body后各新增一次审计；断开的客户端没有可观察HTTP响应，不声称其响应状态差分通过。Rust另验证真实宿主取消；两边均在显式关闭审计后重启/重装配，从同一真实持久目录/MetaStore恢复false，ping保持200且不新增审计。Go histogram实际count增量与请求计数一致，耗时sum不倒退；不要求单调纳秒时钟与Go Unix秒采样数值相等。

未知路径的审计均跳过，但已有Rust fallback返回200，Go返回404；这是现有fallback差异，本任务不更改或宣称未知路径响应兼容。Go源码本来会记录原始query/body，Rust按本任务敏感字段要求不把这些输入交给sink；不宣称Go原始日志文本等价。Go oracle SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8。Go二进制构建信息没有VCS revision，不能声称由当前HEAD构建；实际SHA-256/build info、真实响应、日志与计数保存在原始证据中。

## 来源覆盖

- [source-inventory](source-inventory.md)：任务81行、server/api/router.go与middleware.go、requestutil/context.go/request_info.go、pkg/audit/audit.go及相关测试来源。覆盖HTTP route/backend匹配、开关、结束与取消接线；原Go failpoint测试没有运行，不把其名称当作运行证据。
- [wire-inventory](wire-inventory.md)：各HTTP注册中81的共享审计owner由本边界覆盖。具体领域handler语义仍由各owner验收；真实RPC只验证与HTTP策略隔离，不新增gRPC审计协议。
- [surface-inventory](surface-inventory.md)：C-240/C-243的audit.enable-audit、持久与热更新，审计一次性及脱敏边界。M-182/M-183仅作为真实Go观察手段，主题9/86负责Rust collector/日志实现；不把后端注册、完整指标HELP/TYPE或领域日志实现记作本任务完成。

## 验证记录

所有Cargo从仓库根运行；动态按1–10原子mkdir领取槽位，实际槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`。Rust修改后首先cargo fmt --all，所有子Cargo及fixture继承该路径。

| 确切命令 / 阶段 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo test --locked -p astersql-pd-server --lib task_081_ -- --nocapture`（未接线） | 101 | 1通过、1行为失败 |
| `cargo fmt --all`（最终Rust修改后） | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_081_ -- --nocapture`（最终） | 0 | 3通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture`（最终） | 0 | 269通过、39既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 2170通过、76既有忽略 |

| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/service-audit-oracle.py`（最终） | 0 | 1项真实Rust/Go差分通过、0忽略；Go同目录重启恢复false |

Go前后守卫1101项文件（含ignored/untracked）存在性、SHA-256和Git状态一致；plan.md与基准HEAD字节一致。Go初次/恢复、Rust/Cargo/fixture进程组与隔离目录均已清理；自己的slot-1占用记录和锁目录已释放，编译缓存保留。Python AST、证据文档链接、gzip/JSON往返与git diff --check通过。

[压缩原始证据](task-081-evidence.json.gz)，SHA-256 `d3e9af3f4c59ab4096ffbe74c535c3fb4cf862f8b0f4d87a5952377240408290`，包含红灯、开发失败、最终聚焦/包级/workspace原始日志、真实Go响应/metrics/日志/重启恢复、输入hash、来源映射及完整Go前后守卫。编号任务标记完成后按要求删除；不修改plan.md，原有prompt.md变更不纳入提交。
