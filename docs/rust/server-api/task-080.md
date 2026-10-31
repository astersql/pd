# 任务080：gRPC服务限流

状态：已完成。

基准 `b6e67fc76`；依赖批次79已完成，已核验task-001的主题6准入及任务1–3清单、任务4只读oracle入口。Go/模块与plan.md只读，保留原有prompt.md变更。固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认features，无Cargo依赖变化。

[适配器](../../../pkg/server/grpc_service_limit.rs)在现有PD listener装配，复用[HTTP配置持久化适配器](../../../pkg/server/http_service_limit.rs)、canonical `astersql-pd-ratelimit::Controller`及共享ServiceMiddlewareConfig codec/StorageEndpoint。同一配置锁下发布HTTP与gRPC开关，两个Controller独立。专用POST `/pd/api/v1/service-middleware/config/grpc-rate-limit`校验PD协议标签、更新QPS/并发和持久配置；删除失败恢复旧维度，非删除更新持久失败保留live limiter，保持Go顺序。通用配置只发布配置/开关，专用入口更新live维度。

56个可配置标签固定于协议，独立测试逐项核对descriptor；36个实际准入方法对应Go `server/grpc_service.go` 中调用rateLimitCheck的方法，不将未调用该函数的方法改为受限。标签来自真实URI，拒绝用tonic ResourceExhausted和canonical错误文本。完成guard跨越handler future及响应data/trailer stream；EOF、错误、future丢弃或客户端断开释放并发额度，没有后台worker或私有runtime。

## 行为证据

[独立测试](../../../pkg/server/grpc_service_limit_test.rs)由lib.rs显式注册。初次未接线时测试已编译并运行，配置响应实际`(200, "")`，预期`(200, "gRPC limiter is updated.")`，退出101、1行为失败。后续曾遇tonic into_http泛型未指定的编译错误；它不是红灯证据。另一次故障测试误把QPS删除与并发新增的组合当成非删除更新；Go/Rust均按维度状态位OR判为删除，修正为新建limiter的真实事务失败场景后重跑。

首次完整workspace回归另发现4项runtime取消契约回归：准入阶段取消返回Cancelled覆盖了原Unavailable。修复为保留既有`server not started`状态，先重跑4项pending相关测试及任务22事务取消，再重跑完整验证；原失败日志保留。

四项聚焦测试覆盖真实TCP/tonic/HTTP、共享真实etcd MetaStore：配置成功和无效标签、无维度变化、QPS首次成功/再次拒绝、开关即时生效且保留已消耗token、删除后恢复、HTTP独立、QueryRegion长流占用并发、第二条流拒绝、取消后再次准入、实际持久键及宿主重启后的恢复准入。真实etcd事务错误通过复用任务79测试wrapper，在service_middleware Put附加不存在的lease注入；验证删除失败回滚、通用配置失败不发布以及新增live limiter持久失败不回滚。只把wrapper可见性提升到父测试模块，没有复制领域实现。

[只读supervisor](../../../scripts/grpc-service-limit-oracle.py)启动已有bin/pd-server，Rust测试在同一函数内对两个真实服务运行相同场景，逐项比较HTTP文本、RPC错误码/文本、完整配置及取消后的再准入。成功GetMembers检查真实成员，QueryRegion返回真实NotBootstrapped header；两个实现均实际打开、占用、拒绝和取消长流，不靠mock调用次数。supervisor另重启Go同一数据目录，核对完整配置确实恢复。随机cluster ID仅用于各自有效请求，不归一化限流错误或配置。QPS用低速率/burst避免依赖墙钟等待；可控时钟补充行为沿用canonical领域测试并纳入workspace回归。

Go oracle SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8、无VCS revision，不声称由当前HEAD构建。只复用既有二进制及任务4已审查的只读etcd fixture构建链；不运行make、failpoint启停、Go生成器或Go测试。

## 来源覆盖

- [source-inventory](source-inventory.md)：118、249、263、510、1286–1310、2589、2605。HTTP公共层、shared Controller/codec与gRPC配置接线由本任务当前证据覆盖；共享runner/metrics、审计及Go failpoint清单保持原owner，不宣称本次运行了它们的Go注入差分。补充oracle映射：server/api/service_middleware.go:247–324、server/grpc_service.go:2762–2772、server/server.go:422–430、pkg/errs/errno.go:67–69。
- [wire-inventory](wire-inventory.md)：HTTP-140专用配置入口及共享HTTP-137通用开关/完整配置，真实RPC GetMembers/QueryRegion准入。角色转发复用既有公共层，由包级回归覆盖，未另建服务。
- [surface-inventory](surface-inventory.md)：C-240–247共享结构中gRPC开关与grpc-limiter-config，quoted-bool、完整配置保留、持久恢复和真实限流。HTTP接线沿用79；审计归81；不将共享清单全部行或主题8/9/10记作新行为通过。

## 验证记录

所有Cargo从仓库根运行，`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`；每阶段依次原子mkdir动态领取槽位，实际均为槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁owner记录计划、任务、会话和持有PID，阶段结束后清理自己的锁、保留缓存。Rust修改后首先运行cargo fmt --all。

| 确切命令 / 阶段 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_080_ -- --nocapture`（未接线） | 101 | 0通过、1行为失败 |
| 同一聚焦命令（修正故障场景前） | 101 | 3通过、1失败 |
| `cargo test --workspace --locked`（修复取消契约前） | 101 | runtime 53通过、4失败、36忽略；不是最终结果 |
| `cargo test --locked -p astersql-pd-runtime --lib pending_ -- --nocapture`（修复后） | 0 | 4通过、0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_022_transaction_failure_and_cancel -- --nocapture`（修复后） | 0 | 1通过、0忽略 |
| `cargo test --locked -p astersql-pd-server --lib task_080_ -- --nocapture`（最终） | 0 | 4通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture`（最终） | 0 | 266通过、0失败、39既有忽略 |
| `cargo metadata --format-version 1 --no-deps > /tmp/task080-metadata.json` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |

| `cargo test --workspace --locked`（修复后的最终完整运行） | 0 | 2167通过、0失败、76既有忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/grpc-service-limit-oracle.py`（最终） | 0 | 1项真实Rust/Go差分通过、0忽略；Go同目录重启配置一致 |

Go前后守卫1101项文件（含ignored/untracked），路径、SHA-256及Git状态逐项一致；plan.md与基准HEAD字节一致。最终oracle的输入hash均匹配当前源码，Go初次/重启进程组与隔离目录均清理。全部Cargo/fixture结束后自己的slot-1锁已释放，编译缓存保留。Python AST、文档链接、gzip/JSON往返和git diff --check通过。

[压缩原始证据](task-080-evidence.json.gz)，SHA-256 `5cdd62d2641d9093096af82cff47e7606ce07db1360b5f10af7ece804a117aee`，包含初次行为红灯、开发失败、workspace取消回归、修复后的完整日志、各轮Go原始响应/重启配置、输入hash、来源映射、metadata及完整Go前后守卫。编号任务标记完成后按要求删除；原有prompt.md变更不纳入提交。
