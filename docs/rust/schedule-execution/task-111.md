# 任务111：诊断结果查询

依赖110实际提交为 `772d5e62c`，已核实其生产API、197项表驱动断言、
8个聚焦/192个包级/1741个workspace测试证据；任务109的canonical
`DiagnosticRecorder`、任务107的`SchedulerController`、配置provider和错误catalog
均已存在。本任务新增canonical `astersql-pd-schedule-diagnostic` crate。
两级plan始终只读，不因删除任务文件推定依赖完成。

按用户后续授权，采用**只读Go源码推导 + Rust真实行为/故障验证**替代缺失的
Go运行oracle。未执行Go测试或工具，没有Go运行输出或运行差分声明。
[provenance](logs/task_111/provenance.json)记录计划来源commit、当前来源commit、
四个只读源码SHA-256、fixture摘要及null运行输出；oracle-index没有覆盖本查询的
运行场景。`diagnostic_manager.go`决定分支顺序，`errno.go`决定稳定错误码和消息，
`diagnostic_recorder.go`决定查询时的timestamp与结果聚合，
`tests/server/api/diagnostic_test.go`提供disabled/pending/paused/normal源码断言。
这些均为源码契约，不能误读为实际Go运行证据。

生产路径为[diagnostic.rs](../../../pkg/schedule/diagnostic/diagnostic.rs)，
[边界说明](../../../pkg/schedule/diagnostic/diagnostic.rs.md)。
`Manager::new`借用真实controller、当前配置、宿主实际recorder表，
`get_diagnostic_result(name, unix_seconds)`按顺序返回：全局禁用错误；
调度器missing或按type配置disabled的空Disabled结果；无recorder错误；
无历史错误；canonical recorder最新状态/摘要。使用既有TypedError及RFC catalog，
保留参数和`[RFC code]message`格式。无网络/协议依赖，无私有runtime、时钟或状态复制。
宿主串行管理recorder表并使用同一对象记录/查询；后续facade负责实际循环装配，
本任务不声明已自动接入HTTP或所有调度循环。

[fixture](fixtures/task_111.json)冻结13个有效表驱动案例，13通过/0失败：
全局禁用覆盖present/missing；missing scheduler；按type禁用；不支持诊断的shuffle
和无recorder的leader；空历史；paused/halted/scheduling/normal/pending五个最新状态；
禁用优先于已有pending结果。timestamp包括0、42、u64::MAX；无字段归一化。
全部字段期望来自上述只读源码，不从待测Rust输出反推。

三个独立注册的真实行为测试：

- `task_111_behavior_cases`：上述13个案例，逐字段验证错误码、完整错误文本、
  name/status/summary/timestamp及内部空store map。
- `task_111_live_state_and_ownership`：真实Registry创建scheduler并经MemoryStore
  持久化加入controller；结果副本修改不影响历史，最新paused覆盖pending；
  移除真实scheduler后，即使旧recorder仍存在，也优先返回Disabled；
  随后的全局禁用返回正确错误，历史条数不变。
- `task_111_failed_recording_and_recovery`：真实canonical plan缺失source触发既有
  programmer-error panic，catch后recorder仍空且Manager返回NoDiagnosticResult；
  补齐真实StoreInfo后重新分析，查询恢复为pending、`1 store(s) StoreLowSpace; `、
  timestamp=4，历史仅一条。panic hook输出为预期故障证据，非忽略测试。

红灯[initial](logs/task_111/initial.json)通过可编译的错误行为（跳过全局开关）
执行同一测试：命令exit101、0通过/2失败，错误分类断言实际失败；
[原始输出](logs/task_111/red-disabled.log.gz)保留完整panic。随后恢复开关。
首轮基线在[before-message-fix](logs/task_111/before-message-fix/final.json)，
发现测试期望遗漏canonical TypedError的RFC前缀：聚焦1通过/1失败、workspace
于相同断言失败；生产代码无需修改。修正完整消息断言并补齐独立故障恢复测试后，
重新执行最终全部基线。失败日志未覆盖或删除。

最终命令、退出码、槽位见[final](logs/task_111/final.json)，完整stdout/stderr为
`logs/task_111/final-0.log.gz`至`final-6.log.gz`：

```sh
cargo fmt --all
cargo test -p astersql-pd-schedule-diagnostic --locked --lib task_111 -- --nocapture
cargo test -p astersql-pd-schedule-diagnostic --locked
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

每阶段使用原子mkdir动态领取共享槽位，记录task/plan/session/PID，所有Cargo与子进程
继承绝对`CARGO_TARGET_DIR`。实际槽位1：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2`。所有命令完成后释放自有锁，保留缓存；
失败及中断路径terminate/wait子进程后再释放。无其它任务的锁或缓存被删除。

Cargo.lock只增加本地crate及既有依赖边，无远程版本变化。
全workspace格式化没有任务外Rust差异。[只读审计](logs/task_111/readonly-check.json)
核对1101个Go/模块文件清单、内容SHA-256和Git状态、两级plan哈希一致，
`git diff --check`退出0。工作区原有其它编号任务删除不纳入本次提交。

最终七条命令全部exit0；聚焦3通过/0失败、包级3通过/0失败，workspace
1744通过/0失败、1个既有ignored（含2个compile-fail文档测试）。
[有效数量汇总](logs/task_111/test-counts.json)从原始test result行统计；13个固定
表驱动案例全部通过。全部使用槽位产物的进程已经退出，自有slot-1锁已释放。
