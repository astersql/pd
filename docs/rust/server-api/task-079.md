# 任务079：HTTP服务限流

状态：已完成。

基准 `cfc800659`；已读取批次78及前三项来源清单，默认features，沿用固定kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。只读plan.md与Go/模块不修改。

[HTTP适配器](../../../pkg/server/http_service_limit.rs)在现有listener、角色转发和请求取消域内装配，复用canonical `astersql-pd-ratelimit::Controller`、`ServiceMiddlewareConfig` codec及`StorageEndpoint`。已有dev依赖提升为正常依赖，没有新增库或修改Cargo.lock。服务标签表来自Go `router.go` 的registerFunc/registerPrefix，普通请求按有序模板与方法匹配；按path配置使用注册模板与方法精确查找，空method不会匹配只注册GET的health。请求上下文复用同一标签映射，避免审计与限流各自维护标签。

GET/POST `/pd/api/v1/service-middleware/config`与POST `…/rate-limit`接入真实配置持久键`/pd/<cluster-id>/service_middleware`。配置在该router第一次需要时从共享MetaStore恢复，重建既有领域限流器；QPS和并发实时更新，开关立即生效，两个rate-limit管理标签保持Go allowlist。通用配置更新保留审计和gRPC字段及quoted-bool编码；它只修改持久配置与开关，维度实时变更由专用rate-limit接口驱动，遵循Go。空配置响应保留Go连续写出的文本，且不产生额外持久写入。

完成句柄用Drop包装，在handler返回、读取失败、取消及future丢弃时释放；不新增后台worker或私有运行时。HTTP标签限流不匹配gRPC路径，真实GetMembers RPC仍可成功。Go在删除限流器的持久写失败时恢复旧维度，重建的QPS桶具有新的burst token；测试验证首次放行、随后429。非删除更新的持久写失败不回滚live limiter，与只读Go实现一致。通用配置事务失败不发布候选配置。

## 行为与差分证据

[独立测试](../../../pkg/server/http_service_limit_test.rs)在lib.rs显式注册，全部使用真实TCP、现有宿主与真实etcd，另用真实tonic核验同listener的RPC。首次只有测试和可编译的空模块、尚未接线：POST限流配置得到`(200, "")`，预期`(200, "Rate limiter is updated.")`，实际运行1项行为断言失败，退出101。未把编译错误作为红灯证据。

最终三项聚焦测试覆盖：标签/路径配置及参数拒绝，allowlist拒绝，QPS成功与429，开关关闭/恢复与删除，无关真实RPC，真实事务故障，未完成body请求占用并发、断开连接释放，以及读取实际持久键后宿主重启恢复同一完整配置。事务故障仅在对应Put注入不存在的lease，让真实etcd拒绝提交；没有用mock返回值或调用次数冒充副作用。QPS使用极低速率与burst验证，不靠sleep推进时钟或等待补充token；领域库原有可控时钟测试由workspace回归执行。

[只读Go supervisor](../../../scripts/http-service-limit-oracle.py)复用任务4的二进制校验、Go文件snapshot、隔离目录与进程组清理，启动已有`bin/pd-server`，同组请求逐项比较HTTP状态、响应JSON/文本及完整配置；另关闭Go进程并从同一数据目录重启，核验完整配置确实持久恢复，不归一化错误文本或配置。两端均实际执行断开未完成body连接后再次成功请求的取消案例。Go审计在限流前读取body，故该场景先通过真实配置接口关闭审计，以确保请求已经进入并发限流器；这不是failpoint或Go源码修改。

开发中一次测试误用不存在的Read::key，修正为现有Read::prefix；这不是红灯证据。一次事务测试错误地认为删除失败后的重建桶没有token，按Go实际重建语义修正断言。supervisor先遇到启动期间members返回纯文本，改为等待真实leader JSON；之后发现审计先读body的前置差异，调整为显式关闭审计。首次完整包级另发现旧配置读取取消测试失败：外层限流select用plain-text响应覆盖了handler的JSON取消契约。已将取消select限于admission阶段，handler继续拥有其取消响应；Drop guard仍覆盖handler返回/丢弃，新增配置入口单独处理自身取消，并补齐Go 429的nosniff header。先重跑原失败用例，再重新执行聚焦、差分和完整回归。失败运行与最终运行日志均保存，不把这些失败宣称为通过。

Go二进制SHA-256为`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，构建信息go1.26.8、无VCS revision；不声称它由当前HEAD构建。不调用make、failpoint启停、Go生成器或Go测试入口，不修改Go源码取得证据。

## 来源覆盖与边界

原始相关行冻结于压缩证据：source-inventory 30行，wire-inventory 3行，surface-inventory 225行（包含两个配置原文上下文段，不代表225个行为）。

- [source-inventory](source-inventory.md)：117、249、263、1286–1310、2589、2605。HTTP入口和Controller复用由本任务验证；配置纯类型沿用50，领域runner/metrics共享来源保持原领域归属。
- [wire-inventory](wire-inventory.md)：HTTP-137/138/139三项由真实HTTP、持久恢复和差分覆盖。角色转发沿用公共层，本轮包级回归验证既有角色路径。
- [surface-inventory](surface-inventory.md)：C-240–247共享结构中的HTTP开关和维度、quoted-bool、完整结构保留与恢复在本次运行。审计执行、gRPC限流分别交80/81；共享ratelimit清单中的runner任务/指标和Go failpoint文本不属于本次HTTP接线，也不算新运行的Go故障差分。未宣称主题8/9/10或全部服务行为完成。

## 最终验证

所有命令在仓库根运行，`CARGO_BUILD_JOBS=4`；开发和首次包级使用`RUST_TEST_THREADS=1`，最终修复后的整轮使用`RUST_TEST_THREADS=4`。每阶段依次以原子mkdir动态申请空闲槽位，本任务实际均取得槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；owner记录包含计划、任务、会话及持有PID，finally清理自己的锁，保留缓存。Rust源码修改后先运行`cargo fmt --all`。最终固定Rust/脚本输入后执行以下命令。

| 确切命令 / 阶段 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_079_ -- --nocapture`（未接线） | 101 | 0通过、1行为失败 |
| `cargo test --locked -p astersql-pd-server -- --nocapture`（修复取消契约前） | 101 | 260通过、1失败、39忽略；不是最终结果 |
| `cargo test --locked -p astersql-pd-server --lib task_047_follower_recovery_fault_and_cancel -- --nocapture`（修复后） | 0 | 1通过、0忽略 |
| `cargo test --locked -p astersql-pd-server --lib task_079_ -- --nocapture`（最终） | 0 | 3通过、0忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-service-limit-oracle.py`（最终） | 0 | 1通过、0忽略；18条逐项响应、取消、完整配置一致 |
| `cargo test --locked -p astersql-pd-server -- --nocapture`（最终） | 0 | 262通过、0失败、39既有忽略 |
| `cargo metadata --format-version 1 --no-deps > /tmp/task079-metadata.json` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 2163通过、0失败、76既有忽略 |


最终只读守卫：1101项Go/模块文件（含ignored/untracked）的路径、SHA-256及Git状态与开始逐项一致；plan.md字节与基准HEAD一致。Python AST、证据链接、gzip/JSON往返及`git diff --check`通过。最终Go持久恢复supervisor再次运行同一专用命令，退出0、1项Rust差分通过，Go初次及重启进程组均清理，隔离数据目录删除，输入hash与Go守卫均不变。Cargo与fixture结束后释放自己的slot-1锁，保留编译缓存。

[压缩原始证据](task-079-evidence.json.gz)，SHA-256 `1e767b6e4874eedc9f48279db887a3397cc52f31f6ba277ba39bb5871bbd73b2`。包含初始行为红灯、开发失败、首次包级取消契约回归、修复后的全部命令/退出码/测试数量、各轮Go原始响应与恢复配置、源码映射行、输入hash及完整Go前后守卫。编号任务按要求标记完成后删除；原有prompt.md变更不纳入提交。
