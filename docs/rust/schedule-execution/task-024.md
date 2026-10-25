# 任务024：Operator推进与完成

状态：已完成（验证结果见下）。前置批次23核实提交 `07ac3805e`、
[task-023](task-023.md) 和 canonical MergeRegion/SplitRegion API，前序
OpStatusTracker、OpInfluence、leader/peer/witness/joint步骤均实际接入。
已审查 oracle-index.json 的输出范围；本任务按用户明确授权解除 Go 运行
oracle/运行差分准入。证据为 `source_assertion`，`go_execution=false`；
没有运行 Go operator，也不声称取得跨语言运行差分。Go/模块和 plan.md 只读。

实现：[operator.rs](../../../pkg/schedule/operator/operator.rs)，
[领域说明](../../../pkg/schedule/operator/operator.rs.md)。公开 Operator、
OpStep（12种已实现步骤），提供 Start/Check/CheckSuccess、过期/超时、取消/替换、
confver、未完成/总影响量、优先级及健康检查标志。推进与缓存由执行锁保护，复用
已有状态图和步骤逻辑，不新增 crate、依赖、feature、运行时或持久化。
Controller、Builder、历史及HTTP展示不在本任务范围。

## 冻结来源与测试契约

[fixture](fixtures/task_024.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 及 operator.go/operator_test.go SHA256，
当前源码与该 commit 的字节一致。预期先由只读源码/测试契约确定，不由 Rust 输出生成。
Go 对照来源为 operatorTestSuite 的 TestOperator/TestStart/TestCheckSuccess/
TestCheck/TestInfluence，及 TestAdjustStepCostUsesRegionKeys、
TestPeerInfluenceUsesRegionKeys、TestOperatorCheckConcurrently、
TestOperatorStoreHealthCheck。后两者分别验证并发Check及可随时切换的健康标志；
健康标志不代表 Operator::check 自行验证 store 健康。以上是源码依据，不是本次 Go
测试执行结果；没有字段归一化，时钟仅以显式单调 Clock 控制。

[task_024_test.rs](../../../pkg/schedule/operator/task_024_test.rs) 显式注册4项真实测试：

- `task_024_behavior_cases`：5次固定心跳（包括重复与成功后的旧心跳），逐次比较
  返回步骤、current、confver、success；另验证单次跳过3步、Start重复拒绝、目标元数据、
  默认MEDIUM/ADMIN URGENT/手动HIGH、健康标志切换、step越界及完成时间存在。
- `task_024_terminal_and_clock_boundaries`：7组边界：3秒过期、空操作、Start前推进、
  60秒截止时未完成/已完成两种结果（59秒尚未超时）、取消/替换两种终态。证明完成
  优先于超时、检测超时当次返回未完成步骤、终态不再推进、CREATED不能成功。
- `task_024_influence_and_confver`：3组固定size/keys/cost（1/0/0、1/2/200、21/2/1000），
  检查真实StoreInfluence的size/count/cost、None无缓存、首次缓存固定、不同region及
  None复用、重复累加、当前步confver、移除后的confver、未完成步影响量、完成后为空；
  另验证未推进时跳过心跳已满足的未来步骤，且影响量计算不改变current。
- `task_024_concurrent_heartbeats`：10个线程各100次Check/CheckSuccess，共1000次，
  检查单调推进至3、SUCCESS以及每步首次时间保持不变。

本对象没有存储或外部I/O，因此不添加不适用的MetaStore/mock注入。过期、超时、
空步骤、终态拒绝和旧心跳是状态故障/边界；真实领域步骤已由前置测试覆盖错误输入。

## 红绿与最终验证

[red-missing.out](logs/task_024/red-missing.out)：先注册4项测试，Operator/OpStep API
缺失导致退出101。[red.py](logs/task_024/red.py) 在实现后反转 Check 中的完成判定，
[red-behavior.out](logs/task_024/red-behavior.out) 为4项断言失败，退出101；finally
恢复原生产文件并运行格式化。[red.json](logs/task_024/red.json) 记录命令和实际槽位。
失败证据包含真实错误行为，非仅缺包/注册错误。

[validation.json](logs/task_024/validation.json) 记录最终确切命令、stdout/stderr日志、
退出码和有效数量；[verification.json](logs/task_024/verification.json) 保存只读核对、
fixture哈希及槽位释放证据。

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_024 -- --nocapture` | 0 | 4通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 35通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1398通过、0失败、1既有忽略 |

全部Cargo阶段由 [slot-run.py](logs/task_024/slot-run.py) 原子mkdir动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常/失败/信号路径等待子进程退出后释放自己的锁，保留缓存。
[readonly-validation.py](logs/task_024/readonly-validation.py) 为既有workspace外部
etcd测试复用已有二进制；[Go拦截器](logs/task_024/reuse-go/go) 拒绝实际Go命令，
只复制已核对哈希的该二进制。[provenance](logs/task_024/fixture-reuse-provenance.json)
限定它是外部etcd I/O fixture，不作为本任务operator oracle。没有编译Go。

[verify.py](logs/task_024/verify.py) 逐一核对开始/结束所有Go、模块和plan.md的
路径及内容哈希（包括untracked），并检查 `git diff --check`。格式化只涉及本任务，
Cargo.lock及依赖未变；既有任务10–23删除不纳入本次提交。完成后删除编号任务文件，
保留本证据供批次25核实依赖。
