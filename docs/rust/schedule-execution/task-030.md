# 任务030：见证角色步骤规划

前置批次29提交 `524b159c1`；已核实 task-029.md 的21场景、48个包测试及
workspace 基线证据。复用真实 Builder、Planner、RegionInfo、StoresInfo、配置和
既有 OpStep。生产实现为 [builder_witness.rs](../../../pkg/schedule/operator/builder_witness.rs)，
通过 `Builder::become_witness` / `become_non_witness` 记录目标，由
`build_simple` / `build_joint` 规划；没有新 crate、依赖、feature 或持久化路径。
领域说明见 [builder_witness.rs.md](../../../pkg/schedule/operator/builder_witness.rs.md)。

## 来源与范围

Go 基准 commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`，builder.go 与
builder_test.go 的 SHA256 冻结在 [fixture](fixtures/task_030.json)。来源为
BecomeWitness、BecomeNonWitness、prepareBuild、buildStepsWithoutJointConsensus、
buildStepsWithJointConsensus 和 execBatchSwitchWitnesses。相邻 TestBuild 没有 witness
专项断言，TestMain/TestOperatorBuilderTestSuite 不是本故事运行覆盖。
oracle-index.json 的既有运行输出也不覆盖本故事。依用户后续授权使用
`evidence_kind=source_assertion`、`go_execution=false`；没有 Go 运行 oracle、
没有 Go/Rust 运行差分或 Raft 集群执行声明，不做时间/随机归一化。

21个固定输入/输出场景：14项成功、7项错误（包含 ID allocator 边界故障）。
覆盖单项切换、learner 切换、witness voter 降级/切换/恢复、混合批次、多个 voter
恢复时的 enter/leave、joint 单项回退、添加 witness voter/learner、leader 转移保护、
显式非法 leader、缺失 peer、重复转换、空变更以及两种目标记录 API。
在真实 RegionInfo 上回放心跳：检查完整步骤/目标/身份、最终 peers、输入所有权、
is_finish、conf_ver_changed，核对所有 switch 命令内容；non-witness pending snapshot
必须阻止完成。单成员 V2 自动稳定为显式的 TiKV 心跳输入模型，未启动 TiKV。

WitnessPlan 对两条现有规划路径共享 conversion diff；恢复 voter 在普通路径使用
remove/add learner，在 joint 路径使用 demote；切换后才晋升。joint 按 store ID
批量切换，多个恢复项显式 leave。leader 候选排除待恢复 voter，显式指定时拒绝。
相邻 task029 的 witness-conversion-deferred 场景由 PlanningRequired 更新为真实
BecomeWitness 并回放，不删除或忽略测试。task029 的历史 fixture 哈希/证据仍对应
其提交 `524b159c1`，本次更新在本任务提交中可审查。

## 失败与通过

[red.out](logs/task_030/red.out)：先注册 task_030_behavior_cases，完整 API 可编译，
第一个 become-witness 因 PlanningRequired 与预期成功不符而失败；退出101，
0通过、1失败，不是缺模块或零测试。该测试随后在实现上通过。
[green.out](logs/task_030/green.out) 保留最小实现迭代：核对源码后纠正固定期望中
“目标为 learner 时重加 peer 的 witness=false”（prepareBuild 不走 voter 恢复分支），
随后17项原始场景通过；再补充两种记录 API、恢复目标 leader 拒绝、多个 voter
恢复共21项并添加真实状态/命令回放，[replay.out](logs/task_030/replay.out) 全部通过。
没有从 Rust 待测输出生成 fixture；上述期望修正依据 Go prepareBuild 473–480行。

## 验证与只读边界

所有 Cargo 操作经过 [slot-run.py](logs/task_030/slot-run.py)，按1–10原子尝试，
实际领取槽位1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划/任务/会话/PID，正常、失败、信号中断均等待子进程退出后释放自有锁，
保留缓存。确切命令、退出码、有效测试数见 [validation.json](logs/task_030/validation.json)，
每项 stdout/stderr 保存为 validation-N.out。

workspace 外部 etcd I/O 测试复用前序任务保存的原生二进制，先核对 SHA256；
[只读验证入口](logs/task_030/readonly-validation.py) 的 Go 拦截器仅复制该二进制，
拒绝所有实际 Go 命令。来源见 [provenance](logs/task_030/fixture-reuse-provenance.json)，
实际复制记录见 go-reuse.jsonl；该 fixture 仅为外部 I/O 测试后端，不是 operator oracle。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_030 -- --nocapture` | 0 | 1测试通过；21场景（14成功、7预期错误），0失败/忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 49通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1412通过、0失败、1既有忽略 |

[verification.json](logs/task_030/verification.json) 核对1119份受保护文件：1101份
Go/模块文件与18份 plan.md，SHA256 全部一致；受保护 Git 状态为空，无新增 Go
文件。`git diff --check` 退出0，文档相对链接通过。fixture SHA256 为
`867b2b59c98a58e2d5dd89087151b5becb205ef064e8f8bf493845ad4c5d9d2e`。
已审查差异：只包含转换规划、必要局部接线、相邻回归期望更新、测试与说明/证据；
Cargo.lock 和依赖配置未修改。全部子进程已结束，自有槽位锁已释放，缓存保留。
本任务完成，使用 git-commit 技能只提交本任务实际变更；排除原有编号任务删除，
任务30标记完成后依授权删除其编号文件，不修改只读指南和来源主题。
