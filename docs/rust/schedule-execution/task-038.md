# 任务038：真实操作应用测试工具

状态：已完成（以本文件列出的最终验证证据为准）。

实现：[apply_steps.rs](../../../pkg/schedule/operator/apply_steps.rs)，
边界：[apply_steps.rs.md](../../../pkg/schedule/operator/apply_steps.rs.md)，
测试：[task_038_test.rs](../../../pkg/schedule/operator/task_038_test.rs)。
模块由 lib.rs 的 cfg(test) 注册，仅供本 crate 测试；生产构建无新调度路径。
没有 Cargo 依赖、features 或 Cargo.lock 改动。

## 依赖与来源

前置批次37提交 `ff6eb2202` 和 [task-037.md](task-037.md) 已核实，Operator
状态/推进/终态 API、真实 BasicCluster 与 RegionInfo 可用。本次包级和 workspace
验证重新覆盖前置代码。已审查 oracle-index.json；其中已有 operator concurrent
replace 输出不能代表本工具的执行证据。

依用户后续授权，本任务采用只读 Go 源码契约推导并执行 Rust 行为验证，**没有运行
Go，也没有声称 Go 运行差分**。fixture 明确标记 `source_assertion`、
`go_execution=false`。基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[fixture](fixtures/task_038.json) 冻结 test_util.go、mockcluster.go、operator.go、
region_option.go 的 SHA256。[audit.json](logs/task_038/audit.json) 核实当前来源
与基准字节一致，记录 fixture 哈希与1101个 Go/模块文件的前后内容一致性。

## 行为与验证边界

`apply_operator_step(region, op)` 调用真实 Start/Check，然后克隆并应用基础步骤。
`apply_operator(cluster, op, now_unix_nanos)` 循环至终态，一次发布真实 region indexes，
并刷新新旧成员 store。只有 heartbeat 时间输入可控；没有 mock 调用计数替代状态，
没有持久化或外部 I/O，因此无 MetaStore 后端需求。

Go test_util.go 的 switch **仅支持五类基础步骤**，默认 panic。joint/witness/
split/merge 不能在本任务中擅自扩展为另一套调度实现；本任务逐类验证其未完成时
拒绝执行。已完成步骤由真实 Operator::check 跳过，终态操作不重新执行。
Go 调用方 operator_controller_test.go 的 ApplyOperatorStep/ApplyOperator 使用点
只读参考，不把测试名称视为已运行证据。

五个真实测试全部通过、无忽略：

- `task_038_behavior_cases`：fixture 五组 transfer/add/learner/promote/remove，5成功、
  0失败；逐字段核对 peers、leader、epoch、统计、原始快照不变，以及 influence
  从未完成到空；下一次 Check 才标记 SUCCESS。
- `task_038_apply_full_and_store_status`：四步骤全链；真实 root/role subtree 和 store
  region/leader count、size、heartbeat；移除 store 清零、新增 store 更新、无关 store
  不变；两轮共同成员刷新；旧 size 派生 used；空操作与已取消操作的终态行为。
- `task_038_invalid_membership_panics`：5组错误/故障，含重复 add、重复 learner、缺失
  promotion、移除 leader、pending peer 已存在；panic 消息一致。缺失 remove 和已满足
  add 被 Check 跳过。
- `task_038_unsupported_steps_and_failed_publication`：7类未完成步骤：BecomeWitness、
  BecomeNonWitness、BatchSwitchWitness、ChangePeerV2Enter、ChangePeerV2Leave、
  SplitRegion、MergeRegion 均 Unknown operator step；基础 add 后 merge 出错不发布
  中间 region snapshot。
- `task_038_source_quirks_and_terminal_noop`：witness add/learner 仍创建普通 peer，
  后续调用因重复 peer 拒绝；缺失 transfer 目标使 leader=None；取消后 unsupported
  步骤不执行；promotion 保留 pending 报告、清除 witness 标记。

不归一化错误、membership、状态、影响、epoch 或统计。没有新 feature。
工具和来源一样要求可完成输入，不为永久 pending 状态增加隐式循环上限。

## 失败验证与回归

[red.log](logs/task_038/red.log) 为缺失模块/API 编译失败，exit101。
[mutation.py](logs/task_038/mutation.py) 在同一最终测试组故意将新增 learner 改为
voter；[mutation.log](logs/task_038/mutation.log) 实际3通过/2失败，exit101，
捕获真实 membership 断言与全链错误，证明测试能识别错误行为。finally 恢复源码，
随后重新 fmt 并运行完整通过验证。日志中的 panic 是预期错误捕获，不是被忽略测试。

## 命令与资源

[run.py](logs/task_038/run.py) 各阶段依次原子 mkdir 尝试槽位1–10，记录 plan/task/
session/PID，所有 Cargo 子进程继承绝对路径，退出/失败/中断后释放自己的锁并保留
缓存。所有阶段实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

[validate.py](logs/task_038/validate.py) 执行最终命令链，
[commands.json](logs/task_038/commands.json) 保存每条命令和退出码，
[validation.json](logs/task_038/validation.json) 保存槽位信息，
[validation.log](logs/task_038/validation.log) 合并 stdout/stderr。
workspace 使用前序任务相同的 `RUST_TEST_THREADS=2`，没有新增过滤或忽略。

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_038 -- --nocapture` | 0 | 5通过/0失败/0忽略；基础 fixture 5组通过 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 95通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1458通过/0失败/1个既有忽略 |

有效数量见 [summary.json](logs/task_038/summary.json)，包含2个既有 compile-fail
文档测试。唯一既有 ignored 为 `tso_processes`，需要独立 Go/Rust process binaries，
本任务没有新增 ignored。日志移除多余末尾换行与 mutation.log 并发输出的一处行尾空格，原始哈希、位置与移除内容保存在
[log-normalization.json](logs/task_038/log-normalization.json)，可恢复原始字节。

`git diff --check` 与暂存差异检查通过。仅提交本任务实现、注册、测试、fixture 和
证据；已有其它编号任务删除不纳入。两个只读 plan.md 无变更，Go/模块文件含
untracked 前后内容一致。槽位进程退出后已释放自己的锁，编译缓存保留。
