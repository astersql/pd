# 任务066：Checker与调度诊断HTTP

基准 `988ca2d12bed099afca3acdd788116fd80864a6b`，依赖[任务065](task-065.md)及任务001–004清单/只读入口。仅本任务范围；plan.md只读，协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与接线

来源：[source-inventory.md](source-inventory.md):104、238、241、1512–1517、2575、2579；[wire-inventory.md](wire-inventory.md):139–140、145（HTTP-007/008/013全部三行）；[surface-inventory.md](surface-inventory.md):4736–4737、4742为共享审计来源，审计继续归任务81。诊断配置本身沿用任务48，不重复实现配置领域。

[HTTP适配](../../../pkg/server/http_checker.rs)、[独立测试](../../../pkg/server/http_checker_test.rs)在[lib.rs](../../../pkg/server/lib.rs)显式注册，并挂入[原router](../../../pkg/server/http.rs)。Application/RoleServices透传到[同一调度队列](../../../pkg/runtime/schedule_scatter.rs)，持leader fence访问term coordinator的checker controller及diagnostic manager。没有复制checker、scheduler、MetaStore或引入新worker。

Go只读映射：server/api/checker.go、server/api/diagnostic.go、pkg/schedule/handler/handler.go的GetCheckerStatus/PauseOrResumeChecker/GetDiagnosticResult、pkg/schedule/diagnostic/diagnostic_manager.go，以及pkg/schedule/schedulers/scheduler_controller.go的AllowSchedule/Schedule。七种checker名称沿用canonical pause_controller；delay缺失/负数、整数类型错误及错误JSON metadata、未知checker、未bootstrap和暂停/恢复文案保持Go契约。诊断只允许balance-leader/balance-region；关闭时保留ErrDiagnosticDisabled，不返回固定成功。

必要依赖修复：[生产worker](../../../pkg/runtime/schedule_worker.rs)原来没有填充coordinator的诊断历史。现在按Go限额→halt→pause优先级记录真实门禁结果；[策略适配](../../../pkg/runtime/schedule_policy.rs)把首轮实际balance operators/plans交给既有DiagnosticRecorder，重试不覆盖首轮。scheduler删除后清理历史，重新注册创建新历史；HTTP读取仍使用同一manager。

真实Go差分进一步发现[balance-region](../../../pkg/schedule/schedulers/balance_region.rs)提前跳过已有peer目标，漏记StoreAlreadyHasPeer。现在复用现有ExcludedFilter，并在dry-run记录同一BalanceSchedulerPlan；正常目标准入不变，没有重写调度算法。

## 当前行为证据

初始测试可编译执行，真实HTTP误落入gRPC fallback，Content-Type断言得到application/grpc，退出101、1失败。开发记录还保留整数被通用JSON解析器转为float的失败、JSON错误metadata及冷启动500/406差异、生产balance-region未记录plans的失败，以及Go摘要包含两项StoreAlreadyHasPeer而Rust漏报的真实差分红灯。新增冷启动unsupported场景还纠正了仅由handler源码推导的错误优先级：真实Go路由先返回ErrNotBootstrapped。Go摘要map随机顺序的原始差异亦保留，最终仅归一化完整条目的顺序。均未用编译失败或零测试代替行为红灯。

当前Go/Rust逐项相等的49条HTTP观察涵盖：七种checker初始/暂停/已暂停/恢复，missing/null/negative/fraction/string/unknown-field/bad-JSON/array输入，未知checker，未bootstrap，unsupported scheduler，诊断关闭，halted、paused、限额pending、实际balance-region正常计划摘要及删除后的disabled。去除成功诊断的动态timestamp，并仅对Go map产生的摘要完整条目排序；原因文字及store数量逐项保持。轮询取得的成功诊断先确认timestamp有效，保留status、name、summary、HTTP状态和JSON错误结构。实际region摘要为`1 store(s) StoreScoreDisallowed; 2 store(s) StoreAlreadyHasPeer; `，不是固定服务响应。

Rust真实TCP/etcd额外验证：follower POST暂停后在leader GET观察到相同状态；follower诊断错误与leader一致；checker更新不改变持久配置。挂起真实config事务后，checker请求在同一队列等待，宿主取消后500/server not started，原事务未提交，worker退出。重启复用同一MetaStore后merge暂停清除、enable-diagnostic持久配置保留。PauseController使用显式时间验证截止前/截止时及立即恢复。故障和取消是Rust当前运行证据，不冒充Go故障注入差分。

[只读Go监督入口](../../../scripts/http-checker-oracle.py)复用任务004的binary校验、完整Go哈希/Git守卫和进程组清理，显式执行默认ignored差分项，强制1通过、0失败、0忽略。已有bin/pd-server的build info没有VCS revision，不能声称从当前HEAD构建；实际哈希/build info保留在原始证据。外部etcd fixture不代表Go embedded-etcd拓扑等价。

## 最终验证

状态：已完成。默认features；最终阶段`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=4 GOMAXPROCS=2`，只读监督另设`PD_COMPAT_GO_BIN=$PWD/bin/pd-server`。所有Cargo均使用原子mkdir动态领取的槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；Rust修改后先运行`cargo fmt --all`。前一轮串行回归使用`RUST_TEST_THREADS=1`，其原始输出一并保留。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| 初始`cargo test --locked -p astersql-pd-server --lib task_066_ -- --nocapture` | 101 | 1失败、0忽略 |
| 最终`cargo test --locked -p astersql-pd-server --lib task_066_ -- --nocapture` | 0 | 3通过、0失败、1默认忽略 |
| `python3 scripts/http-checker-oracle.py` | 0 | 1通过、0失败、0忽略；49条Go/Rust观察 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 224通过、0失败、27忽略 |
| `cargo test --workspace --locked` | 0 | 2124通过、0失败、64忽略 |
| `python3 scripts/http-scheduler-config-oracle.py` | 0 | 1通过、0失败、0忽略 |

任务66默认忽略的差分由监督入口显式执行，不把ignored计为行为通过。相邻任务65的160条HTTP及重启配置观察继续相等。

最终1101个Go/模块文件（包含ignored/untracked）的路径、SHA-256及Git状态与开始完全相同；plan.md原字节不变。最终冻结的1288个Rust/Cargo输入在最终聚焦、差分、包级和workspace检查后逐项哈希相同。没有执行make、Go测试/格式化/生成、模块整理或failpoint启停。Python AST、本文与说明文件相对链接、`git diff --check`通过。任务58原删除和prompt.md原修改不纳入提交。

真实服务、fixture、worker及oracle进程组全部结束；自己的持锁进程及锁已清理，槽位缓存保留。原始输出、前后守卫、二进制build info/哈希和清理记录见下方压缩证据。

一次并行workspace回归在未修改的`pkg/utils/syncutil/ordered_single_flight_test.rs`的`singleflight_randomized`中失败（注册随机调用时首轮poll未必Pending，退出101；其余已完成测试2087项通过）。独立同测试随后1通过、0忽略，退出0；没有修改该无关测试或库。前一轮串行workspace为2124通过、64忽略，退出0；并行失败及最终重跑原始输出都保留，不能删除失败记录或把独立通过替代整轮回归。

[压缩原始证据](task-066-evidence.json.gz)，SHA-256 `3a831e5e56737587b19e69764338733384fa76e5945867d2db9164d61e0cba95`。
