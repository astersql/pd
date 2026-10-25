# 任务039：检查器暂停与简单角色修复

状态：已完成（最终检查、计数和只读审计见下列证据）。

生产实现：[role_checkers.rs](../../../pkg/schedule/checker/role_checkers.rs)，
领域说明：[role_checkers.rs.md](../../../pkg/schedule/checker/role_checkers.rs.md)，
测试：[task_039_test.rs](../../../pkg/schedule/checker/task_039_test.rs)。
canonical crate 为 `astersql-pd-schedule-checker`，lib.rs 显式注册生产模块和测试；
Cargo.lock 仅新增 workspace 路径包，无远程依赖升级，无新 features。

## 依赖与来源

已核实批次38提交 `11f9bb6f9`、[task-038.md](task-038.md)，及真实
RegionInfo、StoresInfo、BuilderContext、PeerOperatorFactory、RangeOperatorFactory、
Operator 的公开 API。批次38的 apply_steps 是 operator crate 内 cfg(test) 工具，
不会作为生产依赖导出或复制；本任务通过真实 Operator::start/check 核实外部心跳
完成恢复。已有 placement、config、core 依赖沿 operator 的真实业务路径使用。

已审查 `oracle-index.json`；其运行证据没有覆盖本任务检查器。依用户后续授权，
采用只读 Go 源码和测试契约推导，**没有运行 Go，也不宣称运行差分**。
[fixture](fixtures/task_039.json) 标记 `evidence_kind=source_assertion`、
`go_execution=false`，冻结基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe` 及8份
Go 来源 SHA256。来源覆盖 checker_pause、learner_checker、joint_state_checker、
对应 TestPromoteLearner/TestLeaveJointState、operator 创建/构建、core 分类排序。
[audit.json](logs/task_039/audit.json) 记录 fixture 哈希、当前与基准来源比对和全体
Go/模块文件含 untracked 的前后内容与 Git 状态核对。两个 plan.md 与 HEAD 字节一致。

## 行为、错误与边界

- PauseController：atomic 截止时间，严格 `<` 比较；整秒 TTL，到期恢复；更新覆盖旧值；
  0/负值恢复；Go int64 wrapping；显式时钟边界和多线程更新后的恢复。
- LearnerChecker：按 RegionInfo 分类后的 peer ID 顺序选择；健康晋升、pending 拒绝后
  继续后续成员、乱序 metadata、正常无操作、暂停/到期/恢复、缺少 leader、缺 placement、
  joint origin 拒绝、支持 joint 的单变更仍按普通 promotion 构造。
- JointStateChecker：incoming/demoting/混合 joint、demoting leader 先 transfer、正常
  voter/learner 无操作、暂停/到期/恢复、缺少 leader、恢复跳过 placement。
  无有效目标 leader 时仍生成 leave 的源行为保持不变。
- RoleCheckOutcome.operator 对应 Go 的可选结果，errors 保留 Go 只写日志的类型化
  BuilderError；失败不会凭空产生操作，之前 learner 的错误不妨碍后续成功。

`task_039_behavior_cases` 包含24组冻结案例，逐字段比较描述、region ID、epoch、
kind、priority、step 类型、目标 store/peer ID/witness、transfer 候选及错误。
真实 Operator::start/check 核实所有生成操作初始未完成，预期恢复心跳后到 SUCCESS；
原始区域快照不变。外部心跳的目标状态来自固定期望，不使用待测实现生成期望。
`task_039_pause_boundaries_and_concurrent_resume` 验证暂停边界和并发安全。
没有本任务 ignored、桩、mock 调用次数或新增全局状态；不涉及持久化和外部 I/O，
无需要 MetaStore 故障注入的调用点。

## 失败验证

[red.log](logs/task_039/red.log)：缺失模块/API，exit101。
最初两次通过尝试也保留：[green.log](logs/task_039/green.log)、
[green-2.log](logs/task_039/green-2.log)。它们暴露测试设计的预期错误；回查
Go builder.go 确认纯 promotion 不增加 OpRegion，core/region.go 确认 learners
按 ID 排序后修正。依据保存在
[expectation-corrections.json](logs/task_039/expectation-corrections.json)，未把 Rust
输出反推为 oracle。

最终同一组测试通过三种故意回归证明能识别真实错误行为：
[mutation.py](logs/task_039/mutation.py)、[mutation.log](logs/task_039/mutation.log)、
[mutations.json](logs/task_039/mutations.json)。每次2测试中1通过/1失败，exit101：
忽略 learner 暂停触发 `paused learner: unexpected operator`；joint 优先级改 MEDIUM
触发 HIGH 断言；首个 learner 错误后停止触发后续健康 learner 缺少操作断言。
脚本 finally 恢复生产文件并格式化，之后执行完整最终验证。

首次完整 workspace 回归在未修改的 syncutil `singleflight_randomized` 失败，
`register` 假定 future 首次 poll 必须 Pending 的并发时序断言未满足；
[validation-first.log](logs/task_039/validation-first.log)、
[commands-first.json](logs/task_039/commands-first.json) 保留 exit101。
单独复测同名用例1通过/0失败，exit0，见
[syncutil-retry.log](logs/task_039/syncutil-retry.log)。syncutil 源码无改动；
随后重新执行全部验证，最终结果如下。没有移除测试或扩大实现范围。

## 命令与资源

[run.py](logs/task_039/run.py) 每阶段依次 atomic mkdir 领取槽位1–10，写入 plan/
task/session/PID；所有 Cargo 子进程继承绝对路径，失败或中断终止并等待进程组后
只释放自己的锁，保留缓存。所有实际阶段使用槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

[validate.py](logs/task_039/validate.py)、[commands.json](logs/task_039/commands.json)、
[validation.json](logs/task_039/validation.json)、[validation.log](logs/task_039/validation.log)
保存确切命令、环境、退出码和 stdout/stderr。沿用前置任务 `RUST_TEST_THREADS=2`。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_039 -- --nocapture` | 0 | 2通过/0失败/0忽略；24场景成功 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 2通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1460通过/0失败/1既有忽略 |

最终精确计数见 [summary.json](logs/task_039/summary.json)。唯一既有 ignored 为
`tso_processes`，需要独立 Go/Rust process binaries；本任务未增加忽略或过滤。
不归一化步骤、错误、membership、kind、epoch、优先级或状态；时间是明确输入。
日志仅清理多余末尾空行等 Git whitespace，原始哈希与差异记录于
[log-normalization.json](logs/task_039/log-normalization.json)。

最终 `git diff --check` 和暂存检查通过。Go/模块文件前后无内容或状态差异；
两个只读 plan.md 未修改。仅提交本任务的 Rust、接线、测试和证据；其他任务已有删除
不纳入提交。槽位锁已释放，缓存保留。
