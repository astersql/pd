# 任务055：检查器分派与待处理集合

状态：已完成。

生产路径：[checker_dispatch.rs](../../../pkg/schedule/checker/checker_dispatch.rs)，
[API 与宿主边界](../../../pkg/schedule/checker/checker_dispatch.rs.md)。
`CheckerDispatcher::{check_region,try_add_operators,pause_controller,success_callback}`
组合既有真实检查器，维护共享 TTL pending 与独立 rule pending；不实现巡检循环。

## 依赖与证据性质

依赖54提交 `2f09dfd118109156722a10cd1e3425535df516be`、
[证据](task-054.md)、canonical split-scatter retry API 与39个既有checker测试已核实。
两个 plan.md 只读，已读 ADR-001、porting guide、依赖基线及 oracle-index.json。

用户后续授权解除 Go 运行 oracle 准入限制。本任务依据 **Go只读源码/测试契约推导 +
Rust行为验证** 实施；没有运行本任务调度 Go oracle，没有声称运行差分。
workspace既有测试会只读构建并启动Go etcd辅助fixture，该辅助服务不作为本任务调度运行oracle。
[source-provenance.json](logs/task_055/source-provenance.json)记录实际来源 commit、
Go源码 SHA256；Go输出和输出哈希为 null。

来源契约：

- `checker_controller.go::CheckRegion`：joint → split → rule 或 learner/replica → deny → affinity → merge；首个非空返回；各类限额和缓存；`GetPauseController` 七个名字。
- `checker_controller.go::{tryAddOperators,AddPendingProcessedRegions}`：缺失/已有/no-op/存储预算/队列交付的 pending 转移，严格大于100000的容量判断。
- `rule_checker.go::CheckWithFit`：orphan、逐rule missing/fault/role/location、成功移除pending、最后缓存；只读 `TestPendingList` 提供恢复后移除契约，`TestRuleCache` 提供实际cache契约。
- `affinity_checker.go::RecordOpSuccess` 与 `coordinator.go::NewCoordinator`：operator success callback 才写近期合并状态；`ttl.go`：枚举严格未到期与Get含相等边界。
- `operator/create_operator.go` 的 split factory 拒绝 joint region；仅暂停 joint checker 不会让 split 绕过真实 factory 安全检查。

[fixture](fixtures/task_055.json)冻结38个输入期望，明确标记未运行Go。
没有时间、错误、ID或操作差异归一化；时钟是宿主快照，ID分配固定为1000，
make-up/add-rule 的唯一可用目标为store3，断言实际目标、步骤、epoch、kind掩码、操作描述、优先级与pair数量。

## TDD 与真实行为

先注册 `task_055_behavior_cases`，dispatcher仅返回空结果。
[red.log](logs/task_055/red.log)退出101：`joint-before-split` 断言
`None != Some("leave-joint-state")`，不是缺包或缺注册失败。

实现后新增并验证五个独立行为测试：

1. `task_055_behavior_cases`：38个固定表项，涵盖优先级、七类暂停的路由/实际分派、placement分支、限额0与真实运行数量等于限额、标签、cache命中/失效/epoch拒绝、永久/临时rule失败、恢复移除和分配失败。
2. `task_055_pending_capacity_ttl_and_pause`：100001容量、越界后覆盖拒绝、unchecked插入、remove/clear、179/180秒精确TTL、Get与枚举不同边界、七个pause名称与unknown。
3. `task_055_queue_rejection_existing_and_absent`：真实waiting enqueue/reject、promotion、已有运行操作和缺失region、无新操作时保留pending。
4. `task_055_success_callback`：真实ControllerRecords与heartbeat完成，生成时不缓存、成功双ID缓存、重复burial不通知、取消排除、60秒含相等过期边界。
5. `task_055_store_budget_after_planning`：真实v1令牌由另一region消耗，实际预算拒绝计划操作并加入pending；宿主修改rate后真实limiter重置，成功入队并移除pending。

测试使用真实core、placement、labeler、affinity和MemoryStore初始化、operator/admission/records、TTL及store limiter；仅ID分配故障和宿主时钟可控。
不存在本任务新增的持久化路径或假的MetaStore实现。
扩展测试期间按只读来源修正fixture：affinity移动的实际名称为`affinity-move-region`；
joint暂停后split由真实factory拒绝。预算成本取当前region统计，测试必须把实际region size设为96，不能只改Operator的approximate size。
中间编译/断言失败原样保留在expanded/budget日志中。

[mutation.py](logs/task_055/mutation.py)依次破坏容量严格比较、fit-cache跳过、success状态守卫；
三次全部退出101并命中真实断言，详见[结果](logs/task_055/mutation-results.json)。
脚本finally恢复生产文件，随后重新格式化并执行最终验证。

人工复核发现 Go `handleFilterState` 在临时拒绝时还会移除永久 pending 成员。
补写连续“永久拒绝 → 新store存在但busy”的同一region场景，
[transition-red.log](logs/task_055/transition-red.log)退出101，明确断言rule pending仍为true而期望false。
修复后先处理processed的TTL插入/永久集合移除，再按最终pending决定是否重新加入永久集合；
后者覆盖swap-fit内临时拒绝后再永久拒绝的聚合意图。最终表项也覆盖classic模式先规划再限额时的分配失败。
审查期间主动中断尚未完成的首轮workspace测试（runner退出130），等待子进程退出并释放锁后修改源码；
[中断记录](logs/task_055/workspace-test-interrupted.json)不计为通过，最终完整回归重新执行。


## Cargo 共享槽位与磁盘

[run.py](logs/task_055/run.py)每阶段原子mkdir动态领取1–10槽位，记录任务/会话/PID，
所有Cargo及脚本子进程继承绝对CARGO_TARGET_DIR；失败和中断终止并等待子进程后释放自己的锁。
所有阶段实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

仅新增测试依赖`tokio-util`用于canonical TTL cache的宿主取消token，
`cargo generate-lockfile --offline`后审查并恢复无关pulldown-cmark升级，Cargo.lock仅增加该包依赖边。
后续全部测试/check使用`--locked`。

磁盘可用不足1GiB；在持有槽位1时，仅回收6个30分钟以上的etcd fixture目录，
逐个确认文件名持有PID不存在且所有进程命令均不引用该目录，共484835460字节。
[命令](logs/task_055/fixture-cleanup.json)、[输出](logs/task_055/fixture-cleanup.log)和
[脚本](logs/task_055/cleanup-fixtures.py)保留命令、总量与判断规则。

完整workspace首轮在既有member的`leader_priority_etcd_io_failure`失败（退出101）。
[etcd原始日志](logs/task_055/member-etcd-disk-full.log)明确为WAL预分配`no space left on device`，
当时剩余286MiB。仅清理所持槽位内死进程、无进程引用的fixture数据，以及100个超过6小时、
不再复用的按PID命名fixture可执行文件，共102项3875000288字节。
[重试清理命令](logs/task_055/fixture-cleanup-retry.json)、
[输出](logs/task_055/fixture-cleanup-retry.log)、[脚本](logs/task_055/cleanup-retry.py)保留规则和总量。
两次清理的逐项JSON被同名命令JSON覆盖；不声称仍有逐项清单，
[汇总](logs/task_055/fixture-cleanup-summary.json)如实记录该限制，脚本已改为独立items输出路径。
未删除Cargo debug编译缓存、其它槽位或活跃锁。
空间恢复约3.6GiB后，member失败用例单独重跑退出0、1通过，见
[member-retry.log](logs/task_055/member-retry.log)，随后重跑完整workspace。

## 最终验证

以下为修复恢复后的最终验证，全部退出0；完整stdout/stderr和命令环境分别保存在同名log/JSON。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_055 -- --nocapture` | 0 | 5通过，0失败；38个冻结分派表项及边界/故障序列 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 44通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过，只有既有server弃用字段告警 |
| `RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1502通过，0失败，1既有忽略 |

完整workspace结果包含1500个运行测试及2个compile-fail文档测试；既有忽略为`tso_processes`，未新增忽略。
[最终workspace日志](logs/task_055/workspace-test-retry.log)、
[聚焦最终日志](logs/task_055/focused-final2.log)、[包级最终日志](logs/task_055/package-final.log)、
[测试统计](logs/task_055/test-summary.json)及[全部完成命令](logs/task_055/commands.json)
可独立审查。中断和磁盘失败均单独保留，不计入最终成功结果。

[audit.json](logs/task_055/audit.json)逐一核对1101个Go/模块文件，清单与SHA256未变，
Go Git状态为空，两份只读plan.md与HEAD相同；文档相对链接有效，`git diff --check`通过。
日志仅去除行末空白和末尾多余空行，没有改写状态、错误或测试输出。
提交只含本任务Rust实现/测试/局部注册、单个测试依赖边、fixture、生产说明和证据，
排除原有其它编号任务删除。所有Cargo及测试子进程已结束，自己的槽位1锁已释放，缓存保留。
