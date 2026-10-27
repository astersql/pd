# 任务133：操作结果语义断言

依赖132已核实：提交 `fdf8cc139`、[task-132](task-132.md) 的7项真实测试及
`Manager` 公开 API；本工具的真实直接依赖是已验证的 schedule-operator。
生产实现 [assert_operator.rs](../../../pkg/utils/operatorutil/assert_operator.rs)，
公开 `check_operator`、`OperatorExpectation`、`OperatorMismatch`，十类期望覆盖
Go operatorutil 的公开 Check 工具；完整字段边界见
[说明](../../../pkg/utils/operatorutil/assert_operator.rs.md)。库供 dev-dependency 使用，
未向生产调度 crate 添加断言依赖，无新 feature、外部 I/O 或持久化路径。

按用户后续明确授权，采用**只读 Go 源码/已有测试推导 + Rust 真实行为与故障验证**。
没有运行新的 Go 测试，没有新增 Go 输出，没有声称新的 Go/Rust 运行差分。
[provenance](logs/task_133/provenance.json)冻结来源 commit、依赖提交、fixture及
Go来源和准入 oracle-index SHA-256。oracle-index 的既有运行项不冒充本工具负例证据。

推导依据：`operator_check.go` 全部10个公开工具及 trimTransferLeaders；
`merge_checker_test.go` 的 TestMatchPeers 展示 active/passive merge 与完整前置步骤，
`replica_checker_test.go` 展示增删/迁移 peer，`scheduler_test.go` 展示 learner；
`transfer_witness_leader_test.go` 展示有序 multi-target，
`balance_region_test.go` 的 TestBalanceRegionSchedule1 展示携带 leader 的 peer 迁移。
负例从这些断言的拒绝条件推导，未把列举Go测试名当作运行证据。

[fixture](fixtures/task_133.json)含81个固定输入/布尔期望，24接受、57拒绝。
无时间/随机字段，无归一化。真实步骤构造在 `task_133_test.rs`：
`l`=1→2、空候选；`t`=1→2、[2,3]；`u`候选逆序；`z`=9→1；
`a`=AddLearner至2，`b`至3，`A`同a但peer ID999；`p`=Promote至2，`P`至99；
`r`=Remove自1，`R`自9，`R2`同r但peer ID99；`e/v`=joint enter/leave；
`m/M`=active/passive merge；`x`=不支持的AddPeer。
默认source=1、target=2、required kind=0，fixture显式覆盖非默认值；
merge期望缺省采用同一固定步骤输入，显式expected用于字段投影/拒绝验证。
首次行为运行前校正了fixture的MERGE位与Promote目标投影，期望没有从Rust输出生成。

显式注册2项测试：`task_133_behavior_cases`逐个比较81场景并核对operator的kind、
步骤数量、current_step和status未改变；`task_133_structured_errors`检验缺失操作、
步骤数量、kind错误的精确结构，错误可Display。包含错误变体、顺序、来源、目标、
候选列表、缺kind、额外kind、joint/non-joint、leader trimming、最后leader目标、
target=0通配、无leader、active/passive、空/缺失期望以及Go有意忽略的字段。
类型错误用Result替代Go type assertion panic，未改变接受集合。

失败—通过证据：

- [missing.json](logs/task_133/missing.json)：新增crate/测试注册后缺少实现模块，
  聚焦命令退出101。offline generate-lockfile引入路径包；其意外升级的
  pulldown-cmark已恢复，最终lock只增加operatorutil包及现有依赖边。
- [red.json](logs/task_133/red.json)：实现后临时将kind包含检查替换为true；
  同一聚焦命令实际运行2项、0通过/2失败、退出101。表case1接受了缺LEADER位的
  操作，结构化错误测试也得到Ok而非Kind。finally恢复正确实现，故障未提交。
- [green.json](logs/task_133/green.json)：恢复后2项通过；81案例24接受/57拒绝。
- [final.json](logs/task_133/final.json)：全部最终命令、退出码、测试摘要及压缩日志哈希。
  保存合并stdout/stderr，未过滤故障输出。

Cargo各阶段通过 [verify.py](logs/task_133/verify.py)原子mkdir动态领取共享槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁owner包含计划/任务/会话/PID；子进程继承该环境及GOFLAGS=-mod=readonly。
正常/失败/中断时先等待或结束子进程组，再清理自己的锁；保留缓存。

| 命令 | 退出码/有效测试 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo test -p astersql-pd-operatorutil --locked --lib task_133 -- --nocapture` | 0；2通过/0失败/0忽略（81行为案例） |
| `cargo test -p astersql-pd-operatorutil --locked` | 0；2通过/0失败/0忽略（81行为案例） |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0 |
| `cargo test --workspace --locked` | 0；1843通过/0失败/1既有忽略 |

workspace包含2项既有compile-fail文档测试，无新增忽略，仅既有proto deprecated字段警告。
所有子进程已退出，槽位1锁已释放，缓存保留。
[只读核对](logs/task_133/verification.json)确认1101份Go/模块文件哈希一致，
无新Go文件，Go Git状态为空；两份plan.md与HEAD完全一致。
Cargo.lock无远程版本漂移，格式化未改动其他Rust文件，Markdown相对链接及
`git diff --check`通过。其他任务文件的既有删除未纳入本任务提交。状态：已完成。
