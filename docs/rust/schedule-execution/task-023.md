# 任务023：区域拆分与合并步骤

状态：已完成。依赖批次22已核实提交 `291ce6c91`、[task-022](task-022.md)
及真实 ChangePeerV2Enter/Leave、core、OpInfluence、hbstream API。
已审查 oracle-index.json；已有运行输出不覆盖本任务。依据用户授权解除 Go
运行 oracle/差分准入，本任务使用 `evidence_kind=source_assertion`、
`go_execution=false`，不声称运行 Go 或取得运行差分。Go/模块与全部 plan.md 只读。

实现：[step_range.rs](../../../pkg/schedule/operator/step_range.rs)，
[领域说明](../../../pkg/schedule/operator/step_range.rs.md)。公开 MergeRegion、
SplitRegion 的完成、confver、进度、影响量、超时、命令和 Display。
复用真实领域模型、快步骤超时与心跳命令，不增加依赖、feature、持久化或外部 I/O。

## 冻结来源与行为

[fixture](fixtures/task_023.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、step.go/step_test.go 的 SHA256；
当前字节与基准 commit 逐一核对。期望来自 step.go 的 MergeRegion/SplitRegion
及 fastStepWaitDuration。已检查相邻 TestOperatorStepTestSuite：step_test.go
不存在 MergeRegion/SplitRegion 案例，不能将其 suite 名称冒充这些行为的测试证据。
所有新增期望均明确为源码推导，没有字段归一化。

[task_023_test.rs](../../../pkg/schedule/operator/task_023_test.rs) 显式注册3项测试：

- `task_023_behavior_cases`：6行固定范围/epoch输入 × 主动/被动2种模式，12组，
  比较 merge/split 完成、confver、两种 health flag 下的进度结果；包含原范围不变、
  仅epoch变、左/右/双边变化、二进制前缀；另有空范围、仅ID/epoch存在性变化、
  nil目标命令及 passive nil目标 panic 的程序员错误边界。
- `task_023_commands_and_timeout`：主动/被动 × V1/V2 × 4种 policy，共16组
  policy/命令组合；完整比较命令，包括目标epoch、peers、范围，保持重复、无序、
  空和二进制split keys。6组固定大小 × 2种模式，共12组timeout，覆盖最低值、
  截断和merge有符号溢出，比较timeout_nanos与Duration位视图。也覆盖空keys及Display。
- `task_023_influence_counts_and_fault_inputs`：2种leader输入 × 主动/被动模式，
  4组真实RegionInfo状态，比较每个store全部影响量字段。非零region size、learner、
  witness、重复store、peer-ID和store-ID不一致及nil leader/零peer-ID都覆盖；
  另检查空peers及计数溢出。size和witness/cost字段不随范围步骤变化。

这些步骤不访问MetaStore或外部I/O，因此无适用持久化故障注入点。未知policy、
畸形peer/leader、缺失target和overflow是本任务领域故障/边界输入；不引入mock。

## 红绿验证

[red-missing.out](logs/task_023/red-missing.out)：先注册测试后缺少API，退出101；
同时暴露测试对ProviderSnapshot泛型及不存在ApproximateSize选项的误用，已修正为
真实 `ProviderSnapshot<()>` 和 `stats_mut` API。
[first-green.out](logs/task_023/first-green.out)：最小实现后三项通过。
[red.py](logs/task_023/red.py) 将split的start-key比较反转，
[red-behavior.out](logs/task_023/red-behavior.out) 在原范围不变案例产生真实断言失败，
退出101；finally恢复源码并格式化。[red.json](logs/task_023/red.json) 保存结果。
这证明测试可以识别错误行为，失败证据不只依赖缺包/缺注册错误。

[validation.json](logs/task_023/validation.json) 和validation-*.out保存确切命令、
stdout/stderr、退出码及测试计数；[verification.json](logs/task_023/verification.json)
保存只读核对、fixture哈希和槽位释放记录。验证结果见下表。

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_023 -- --nocapture` | 0 | 3通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 31通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1394通过、0失败、1忽略 |

全部Cargo阶段由 [slot-run.py](logs/task_023/slot-run.py) 原子mkdir动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常/失败/信号路径等待子进程结束，只释放自己的锁并保留缓存。
[readonly-validation.py](logs/task_023/readonly-validation.py) 使用已有外部etcd I/O
fixture；[拦截器](logs/task_023/reuse-go/go) 拒绝实际Go命令，仅复制既有二进制，
[provenance](logs/task_023/fixture-reuse-provenance.json) 记录其哈希与来源。
该二进制不是operator oracle，没有编译Go。

[verify.py](logs/task_023/verify.py) 核对任务开始/结束全部Go、模块、plan.md的清单与
内容哈希，包括untracked；无Go或模块差异、无依赖变化，格式化差异仅限本任务。
既有任务10–22删除不纳入提交。本任务完成后删除编号文件，保留此稳定证据供批次24使用。

最终fixture provenance补齐后，[final-focused.out](logs/task_023/final-focused.out)
再次运行同一聚焦命令，退出0、3项通过。workspace唯一忽略为既有测试，未新增忽略。
