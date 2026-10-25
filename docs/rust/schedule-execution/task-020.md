# 任务020：删除与降级副本步骤

状态：已完成。依赖批次19由提交 `7726facff`、[task-019](task-019.md)
及其红绿/全 workspace 证据确认；本任务全包验证再次覆盖全部前置操作器行为。
已核实 `oracle-index.json`，没有将其它行为的 Go 输出当成本任务运行证据。
用户已解除 Go 运行 oracle/运行差分准入限制；本任务明确为
`evidence_kind=source_assertion`、`go_execution=false`，期望来自只读 Go 源码契约。

实现：[step_remove.rs](../../../pkg/schedule/operator/step_remove.rs)，
[领域说明](../../../pkg/schedule/operator/step_remove.rs.md)。公开 `RemovePeer`
的完成、confver、进度检查、影响量、超时、命令及 Display；类型化错误为
`RemovePeerError`。`DemoteVoter` 提供完成、confver、超时、命令及 Display，
保留其允许 leader 降级的 joint-consensus 子步骤边界；Go 没有独立的
CheckInProgress/Influence 方法，故未虚构这两个接口。复用真实 core、config、
storelimit、OpInfluence 和 hbstream；没有持久化/I/O、新依赖或 Cargo.lock 变化。

## 固定来源与案例

[fixture](fixtures/task_020.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、5份只读来源文件的 SHA256、
原始输入和源码推导期望。当前字节与该 commit 逐一核对。
来源为 `step.go` 中 RemovePeer/DemoteVoter 的全部方法及 fast timeout/命令辅助方法，
`region.go` 的 peer-local size/keys、`influence.go` 和 storelimit 成本表。
相邻 `step_test.go` 的 ChangePeerV2Enter/Leave 覆盖联合角色背景；
没有将其描述为独立 RemovePeer/DemoteVoter 测试或已运行 oracle。
无随机或时间采样，无字段归一化；size/keys 保留有符号输入。

显式注册 [task_020_test.rs](../../../pkg/schedule/operator/task_020_test.rs) 的3项测试：

- `task_020_behavior_cases`：11行，包含缺 peer、Voter/Learner/IncomingVoter/
  DemotingVoter/未知角色、替换 ID、witness、零 ID；分别比较删除/降级完成和
  confver，另比较 witness 降级及未指定删除 ID 的 confver。尤其保留“替换 ID
  confver 已完成、但删除步骤尚未完成”和“缺 peer 降级 confver 已完成、但降级
  步骤尚未完成”的差异。
- `task_020_commands_and_timeouts`：5种角色×2种 witness×2种存在状态×2种
  命令格式×2类步骤，共80次完整 Operation 比较；删除使用当前 peer ID/角色，
  缺 peer 保留 None，降级合成预期 Learner。检查字符串，7组超时×2步骤共14断言，
  包括负 size、截断边界和 i64 溢出。
- `task_020_costs_and_faults`：7组大小/keys×轻量×down store×witness×peer
  存在，共112组真实 StoreInfluence 完整比较；包括空区域、20MiB边界、down cap、
  缺 peer 的数据 fallback、witness 不收费。另验证3种 leader×2种 health 参数的
  6组进度结果，缺失 store 不阻止删除、nil leader/store0 的拒绝、错误枚举及文本、
  连续 heartbeat 角色 Voter→DemotingVoter→Learner（leader 也可降级）、重复删除
  完成判定，以及 region size/count/witness count 的3个有符号溢出断言。

## 红绿与验证

[red-missing.out](logs/task_020/red-missing.out) 保存初次缺 API 的编译失败，退出101。
[red.py](logs/task_020/red.py) 在实现后将删除完成条件故意改为“peer ID 已替换也完成”，
同一固定行为测试在 `wrong-voter` 行断言失败，退出101；
[red.json](logs/task_020/red.json)、[输出](logs/task_020/red-behavior.out) 保留确切结果。
finally 恢复原始实现并格式化；[恢复验证](logs/task_020/after-red.json) 再次通过
3项聚焦测试、23项包测试和格式检查。测试期望未从待测 Rust 生成。

[validation.json](logs/task_020/validation.json) 和 `validation-*.out`
保存完整 stdout/stderr、命令、退出码和实际数量：

| 命令 | 退出码 | 有效测试 |
| --- | ---: | ---: |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_020 -- --nocapture` | 0 | 3通过 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 23通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1386通过、0失败、1既有忽略 |
| `git diff --check` | 0 | 通过 |

全部 Cargo 阶段使用 [slot-run.py](logs/task_020/slot-run.py) 原子 mkdir
动态领取共享槽位，实际均为槽位1，绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
子进程结束后释放自己的锁并保留缓存；失败/信号路径也等待子进程退出后清理。

[readonly-validation.py](logs/task_020/readonly-validation.py) 复用批次19已审查的
只读测试路线；[Go拦截器](logs/task_020/reuse-go/go) 只复制已有外部 etcd fixture
二进制、拒绝实际 Go 命令。该二进制仅提供 workspace Rust 存储测试的外部 I/O，
不是操作器 oracle。其 SHA256/来源及每次请求保存在
[provenance](logs/task_020/fixture-reuse-provenance.json) 和
[复用日志](logs/task_020/go-reuse.jsonl)。

[verify.py](logs/task_020/verify.py) 比较任务开始的全部 Go/模块/plan 文件清单及哈希，
[verification.json](logs/task_020/verification.json) 保存1119份文件核对、fixture 哈希、
命令数量、槽位及释放结果。Go/模块没有任何新增差异，两份只读 plan 未改动。
审查格式化只修改本任务 Rust 文件；仅提交本任务实现、接线、测试和证据，排除已有
任务10–19文件删除。编号任务20完成后删除，不修改来源主题或任何 plan.md。
