# 任务022：联合共识进入和退出步骤

状态：已完成。依赖批次21已核实提交 `b55841236`、[task-021](task-021.md)
与真实 Rust API。已审查 oracle-index.json，其已有运行输出不覆盖本任务；依据用户
明确授权解除 Go 运行 oracle/差分准入，本任务 `evidence_kind=source_assertion`、
`go_execution=false`，所有期望来自只读 Go 源码和测试，没有运行 Go oracle，
没有声称运行差分。Go/模块文件与全部 plan.md 保持只读。

实现：[step_joint.rs](../../../pkg/schedule/operator/step_joint.rs)，
[领域说明](../../../pkg/schedule/operator/step_joint.rs.md)。公开 ChangePeerV2Enter、
ChangePeerV2Leave、JointStepError；完成、confver、进度、无影响量、超时、命令和
Display 已接线。复用真实 core 与前序 PromoteLearner/DemoteVoter，不引入依赖、
协议类型、持久化或外部 I/O。timeout_nanos 保留 signed overflow，Duration 为位视图。

## 来源与行为场景

[fixture](fixtures/task_022.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 及 step.go、step_test.go、peer.go、
region.go 的 SHA256，当前字节逐个与基准 commit 核对。相邻 Go 测试为
TestChangePeerV2Enter、TestChangePeerV2EnterWithSingleChange、TestChangePeerV2Leave；
仅用作只读行为契约，不声称已运行它们。不归一化任何字段。

[task_022_test.rs](../../../pkg/schedule/operator/task_022_test.rs) 显式注册2项测试：

- `task_022_behavior_cases`：36行输入 × 2种 pending 状态 × 2个步骤，144组结果，
  每组同时比较 confver、finish 和精确错误文本，共432项断言。25行覆盖两目标的
  Voter/Learner/IncomingVoter/DemotingVoter/Unknown 全组合；另11行覆盖目标缺失、
  替换 ID、额外 joint peer、降级 leader、原子性错误优先级与 witness 转 data。
- `task_022_commands_empty_single_and_cost`：完整 Operation 比较进入命令的顺序、
  重复成员与 witness 位；两个步骤 V1 返回 None，退出 V2 为空 envelope；6组大小
  比较 timeout_nanos 及 Duration 位视图；影响量保持完整已有 store snapshot；
  两种 Display、单 promotion/demotion 直接到稳定角色、空成员步骤、额外 joint
  peer 和 Go nil getter/ID零边界。超时基数复用已验证的 TransferLeader fast timeout。

所有测试使用真实 RegionInfo、BasicCluster、配置和 OpInfluence。角色/leader/部分
heartbeat 为领域故障输入；本步骤不访问 MetaStore，故无持久化故障注入适用点。

## 红绿与验证

[red-missing.out](logs/task_022/red-missing.out) 为先注册测试后缺 API 的编译失败，退出101。
[first-green.out](logs/task_022/first-green.out) 记录接入时配置泛型未指定的编译失败，
修正测试 ProviderSnapshot 的类型参数后 [green.out](logs/task_022/green.out) 2项通过。
[red.py](logs/task_022/red.py) 故意删除 Leave::is_finish 的全区域 joint 检查，
[red-behavior.out](logs/task_022/red-behavior.out) 在固定 other-joint-after 和空操作边界
断言失败，退出101；finally 恢复生产源码并格式化。该脚本最初只在 stdout 查找断言
文本导致驱动自身失败，已修正为同时审查 stderr；真实测试失败记录未被当成通过。

[validation.json](logs/task_022/validation.json) 及 validation-*.out 保存命令、
stdout/stderr、退出码和有效测试数：

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_022 -- --nocapture` | 0 | 2通过 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 28通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1391通过、0失败、1既有忽略 |
| `git diff --check` | 0 | 通过 |

全部 Cargo 阶段由 [slot-run.py](logs/task_022/slot-run.py) 原子 mkdir 动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常/失败/信号路径等待子进程结束后只释放自己的锁，保留编译缓存。
[red-driver.out](logs/task_022/red-driver.out) 记录修正驱动后重复破坏性验证、恢复源码、
同一聚焦测试再次2项通过。完整基线与该恢复后源码相同。

[readonly-validation.py](logs/task_022/readonly-validation.py) 复用前序已有 etcd fixture，
[拦截器](logs/task_022/reuse-go/go) 拒绝所有实际 Go 命令，仅复制已有二进制。
[provenance](logs/task_022/fixture-reuse-provenance.json) 和 [复用记录](logs/task_022/go-reuse.jsonl)
保留外部 I/O 夹具来源/哈希；该夹具不是 operator oracle，未编译 Go。

[verify.py](logs/task_022/verify.py) 核对开始/结束的全部 Go、模块、plan.md 清单与哈希，
[verification.json](logs/task_022/verification.json) 保存 fixture 哈希、只读核对结果、
测试计数和槽位释放状态。无 Go 或模块文件差异，无新增依赖；格式化仅触及本任务。
既有任务10–21删除排除在提交之外；本任务验证完成后删除编号文件，不改两份只读计划。
