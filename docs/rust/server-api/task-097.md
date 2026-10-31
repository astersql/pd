# 任务097：独立进程信号关闭

基准 `842dbdd10`；依赖[任务096](task-096.md)及其传递依赖已满足。
复用 `cmd/pd-server/main.rs` 的进程入口、信号处理和 `PdNode::shutdown`，
现有生产行为已覆盖，无需另建 process_shutdown.rs 或改写领域层。
新增[独立测试](../../../cmd/pd-server/process_shutdown_test.rs)，
在[lib.rs](../../../cmd/pd-server/lib.rs)显式注册；四项现有 workspace 依赖仅用于测试。

## 来源及退出契约

[source-inventory.md](source-inventory.md) 第135行任务97、第151行main.rs公开入口、
第992/993行main.rs/main_test.rs来源全部对应本次证据。
[wire-inventory.md](wire-inventory.md) 无任务97专属协议；使用已有GetMembers、Bootstrap、
AllocID、TSO、RegionHeartbeat、SyncRegions、WatchGlobalConfig和HTTP health。
[surface-inventory.md](surface-inventory.md) 的main.go/CLI及main_test.rs共享来源沿用
90/91/92，不增加CLI、配置或指标；信号关闭证据由本任务补充。
清单的历史manifest哈希不作为本次运行通过数。
包级/workspace数量沿用libtest统计；既有CLI子进程入口辅助测试默认空返回，
不将该辅助项计为任务97行为证据，本任务有效默认行为为2项，另有显式差分1项。

只读Go oracle：`cmd/pd-server/main.go:296–321` 注册信号，取消context，调用
`svr.Close()`，SIGTERM退出0，其他信号（含SIGINT）退出1。
Rust `main.rs::shutdown_signal/run_with_logging/main` 已遵守同一契约。
任务中的“SIGINT退出成功”按真实Go契约解释为完成有序关闭，不将其改为退出0；
**两实现SIGTERM=0、SIGINT=1，均正常回收资源**。本次真实运行证实此区别。

## 行为证据

`task_097_behavior`：分别发送SIGTERM/SIGINT，保持真实TSO、heartbeat、sync、global
watch四类流和客户端发送端存活。真实Bootstrap、AllocID、HTTP health及流首响应确认服务
已就绪；同一MetaStore有真实带lease的leader key和持久global config写入。
关闭后四流在有限时间内终止、子进程在12秒期限内退出并wait回收；端口拒绝新连接且可
重新bind。Rust外部MetaStore确认leader key删除、旧lease失效、持久写入保留。
随后同端口、同身份、同存储重启，cluster ID不变、AllocID递增、持久写入仍在；再次TERM退出0。

默认聚焦测试通过已有CLI测试入口重新执行当前测试可执行文件，在**独立OS进程**中调用
真实main，包含独立Tokio executor和真正信号处理，不模拟shutdown future。
差分入口另用当前构建的实际`debug/pd-server`二进制重复Rust场景，避免仅用测试入口验收。

`task_097_startup_failure`：真实listener冲突导致退出1、wait回收，原进程HTTP/RPC仍健康；
原进程关闭后，同一身份/端口的启动可成功重试。另验证Rust先绑定listener后连接不可达
MetaStore失败，退出1且端口释放，再用正确endpoint启动成功。
`task_097_readonly_differential` 在只读supervisor `--case process-shutdown` 下显式运行，
对真实已有Go二进制与当前Rust二进制分别执行启动冲突/重试和两种信号场景。

实际流终态：Go TSO/heartbeat=Unknown、sync=Unavailable、watch=EOF；
Rust TSO/heartbeat/watch=EOF、sync=Unavailable。两者均有限终止；不将不同终止状态声称为
逐字节等价（沿用任务095已记录的传输边界）。持久写入、身份、端口及leader复用均验证。
Go有独立embedded-etcd，Rust使用真实fixture外部etcd；不宣称主题10嵌入拓扑等价。
worker显式join路径复用任务095/096，Rust的PD stopped日志发生在shutdown完成后；
本次进程退出、流终止、lease释放及重启给出进程边界证据，不把源码扫描当动态差分。

首轮测试可编译，退出101（1通过/1失败/1默认忽略）：启动重试测试错误地换了peer身份。
第二轮退出101（同样计数）：修正peer后，测试仍向旧保留端口查询就绪。
修正测试地址后通过。这两次是**测试自身错误**，不冒充生产行为TDD红灯；
已有生产行为无需人为制造失败。最终又补充不可达MetaStore错误场景并重新验证。

## 验证记录

仓库根执行、默认features；`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=4`。
每阶段原子mkdir动态领取 **slot-1**；实际
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
差分环境为`PD_COMPAT_GO_BIN=$PWD/bin/pd-server`、
`PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server`，继承同一槽位。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| 初轮 `cargo test --offline -p astersql-pd-cmd-pd-server --lib task_097_ -- --nocapture` | 101 | 1通过、1失败、1默认忽略；锁文件仅增加现有测试依赖引用 |
| 第二轮 `cargo test --locked -p astersql-pd-cmd-pd-server --lib task_097_ -- --nocapture` | 101 | 1通过、1失败、1默认忽略；测试地址错误 |
| 最终 `cargo fmt --all` | 0 | 格式化 |
| 最终 `cargo test --locked -p astersql-pd-cmd-pd-server --lib task_097_ -- --nocapture` | 0 | 2通过、1默认忽略；差分另行显式运行 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 当前实际进程二进制 |
| 最终 `bash scripts/run-pd-compat-readonly.sh --case process-shutdown` | 0 | 1通过、0忽略，含Go/Rust启动故障和四套信号/重启场景 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --nocapture` | 0 | 18通过、1默认忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-cmd-pd-server` | 0 | 包级检查 |
| `cargo check --workspace --locked` | 0 | workspace检查 |
| `cargo test --workspace --locked` | 0 | 2223通过、79默认忽略（含文档测试） |

[压缩证据](task-097-evidence.json.gz)保存完整命令/退出码/输出、Go二进制build info/hash、
只读supervisor报告、Go文件清单前后哈希与Git状态、来源映射、清理核验。
已有Go二进制构建信息无VCS revision，不声称由当前HEAD构建；未构建Go oracle，
只复用既有只读fixture源码的固定只读构建路径，不运行make/failpoint/Go格式化或模块整理。

Go/模块文件（含ignored/untracked）共1101个，开始/结束路径、SHA-256和Git状态完全相同；
只读plan.md哈希不变。自有测试进程、工作目录及slot-1锁已清理，编译缓存保留。
保留原有任务94/95/96删除和prompt.md变更，均不纳入本任务提交。
Python AST、文档链接与`git diff --check`检查通过。
最终收紧lease断言为明确的LeaseNotFound（拒绝把任意后端错误视为回收成功），随后重跑聚焦、差分、包级及格式检查通过；
workspace完整回归在此断言收紧前运行，生产代码始终未改动。

状态：已完成。
