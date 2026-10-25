# 任务019：增加与晋升副本步骤

状态：已完成。已核实依赖批次18提交 `a1f2210ec`、
[task-018](task-018.md) 的红绿/全量证据及 canonical crate API；
本任务包级测试包含全部前置操作器测试。

用户已解除 Go 运行 oracle/差分准入要求。本任务 `evidence_kind=source_assertion`、
`go_execution=false`，期望来自只读 Go 源码与测试，不声称 Go/Rust 运行差分。
已检查 `oracle-index.json`，其中 controller 并发等运行结果不覆盖本任务三类步骤，
未将其用作增加/晋升的运行证据。

实现：[step_add.rs](../../../pkg/schedule/operator/step_add.rs)，
[领域说明](../../../pkg/schedule/operator/step_add.rs.md)。公开 `AddPeer`、
`AddLearner`、`PromoteLearner` 的 `is_finish`、`conf_ver_changed`、
`check_in_progress`、`influence`、`timeout`、`get_cmd` 与 Display；
类型化错误为 `PeerStepError`。复用真实 core、配置、hbstream、OpInfluence；
没有持久化和 I/O，没有新依赖、feature 或 Cargo.lock 变化。
只处理三类步骤及其已有 witness 属性，不实现 witness 转换/joint 操作。

## 来源与固定场景

[fixture](fixtures/task_019.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、源码 SHA256、原始输入和源码期望。
来源 `step_test.go` 的 `TestAddPeer`、`TestAddLearner` 提供增加前后/缺 store 契约；
`step.go` 中三类完整方法、`validateStore`、超时/命令辅助方法提供其余边界。
`region.go` 的角色/pending 分类、空区域判定以及 influence/storelimit/store
源码提供成本与健康边界。当前源文件与该 commit 的字节哈希已核对。
无随机输入或字段归一化；显式 Unix 纳秒时间和有符号 region size 保留。

显式注册 [task_019_test.rs](../../../pkg/schedule/operator/task_019_test.rs) 的4项测试：

- `task_019_behavior_cases`：12行固定 fixture，分别断言三类步骤的完成和 confver；
  包括缺 peer、普通/联合/未知角色、错误 ID、pending voter/learner/异类 pending、
  witness 不匹配；每行还检查2种格式的增加命令抑制及晋升命令存在。
- `task_019_commands_and_costs`：witness×v1/v2×3步骤共12个完整命令比较；
  字符串检查；6组大小/keys×轻量×witness×3个 SendStore×2种增加类型共144组
  真实影响量与成本快照；晋升不影响快照。包括20MiB阈值、空区域、小区域、
  负 size、源目标同店、无源。7组有符号/溢出超时分别检查3类步骤，共21断言。
- `task_019_validation_faults`：6组 store 存在/健康/严格10分钟和30分钟边界/
  未来 heartbeat×3组 ID/角色，18组同时检查增加步骤错误优先级与晋升行为。
  增加检查保留冲突 ID 与消息；晋升不检查 store 健康。另检查缺 peer 和零 ID
  nil-safe confver/检查语义。
- `task_019_heartbeat_and_boundaries`：真实快照从缺失到 pending 到完成；
  witness 匹配、learner 匹配，4种非 learner 的拒绝与 AddPeer 接受，
  直接比较错误枚举，region/witness/count 有符号累加溢出。

首次成本断言期望曾错误地将 size=0/-1、keys=0 视为非空区域。核查只读
`IsEmptyRegion(size <= 1 && keys == 0)` 后修正为成本0；
[初始失败与审查](logs/task_019/initial-expectation-review.out) 保留原始输出。
此期望纠正不作为实现的行为红灯证据。

## 红绿与验证

[缺API红灯](logs/task_019/red-missing.out) 退出101，仅证明初次缺失 API。
[red.py](logs/task_019/red.py) 在实现后临时删除 AddPeer 的 pending-voter guard，
同一 `task_019_behavior_cases` 在固定 `pending-voter` 行断言失败，退出101；
[red.json](logs/task_019/red.json)、[输出](logs/task_019/red-behavior.out) 保存具体命令和结果。
finally 恢复实现，随后[聚焦测试再次通过](logs/task_019/after-red.json)（4通过、退出0）。没有从 Rust 生成期望。

[validation.json](logs/task_019/validation.json)、`validation-*.out` 保存完整 stdout/stderr、
确切命令、退出码和数量：

| 命令 | 退出码 | 有效测试 |
| --- | ---: | ---: |
| `cargo fmt --all` | 0 | 格式化通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_019 -- --nocapture` | 0 | 4通过 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 20通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1383通过、0失败、1既有忽略 |
| `git diff --check` | 0 | 通过 |

[readonly-validation.py](logs/task_019/readonly-validation.py) 复用批次18已验证的
只读验证路线；[Go拦截器](logs/task_019/reuse-go/go) 只复制已有 etcd fixture
二进制并拒绝其它 Go 命令。该二进制仅为 workspace 真实 Rust 存储测试提供
外部 etcd I/O，不是 operator oracle；
[provenance](logs/task_019/fixture-reuse-provenance.json) 和
[复用请求](logs/task_019/go-reuse.jsonl) 保存源路径、SHA256及构建请求。

全部 Cargo 阶段由 [slot-run.py](logs/task_019/slot-run.py) 原子 mkdir 动态领取。
实际槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；
子进程结束后释放自己的锁，保留缓存，失败和信号路径使用 finally 清理。
[verify.py](logs/task_019/verify.py) 比较任务开始的 Go/模块/plan 全量清单与哈希；
[verification.json](logs/task_019/verification.json) 保存核对结果、fixture SHA256、
实际测试数和槽位。两份 plan 未修改；Go/模块没有任何新增差异。
只提交本任务代码、注册、说明和证据，排除原有任务10–18文件删除。
