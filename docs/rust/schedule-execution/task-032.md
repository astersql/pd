# 任务032：范围与见证操作工厂

状态：已完成。前置批次31提交 `6da582a82`，其48场景、50个包测试与 workspace
基线证据已核实；Builder 普通、joint、witness 和 leader 选择接口均可用。
生产入口 [create_range.rs](../../../pkg/schedule/operator/create_range.rs) 提供
`create_split_region_operator`、`is_region_match`、`RangeOperatorFactory` 的
merge/scatter/leave-joint/witness/move-witness 方法。内部恢复入口位于
[builder_joint.rs](../../../pkg/schedule/operator/builder_joint.rs)，复用 canonical
Planner，不复制 leader 评分算法。边界见
[create_range.rs.md](../../../pkg/schedule/operator/create_range.rs.md)。

## 来源与行为证据

Go 源码基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[固定 fixture](fixtures/task_032.json) 保存 create_operator.go、相邻测试和
builder.go 的 SHA256、原始输入、预期完整步骤/kind/错误/最终 peers 与 leader。
全部是 `evidence_kind=source_assertion`、`go_execution=false`，无字段归一化。
依用户后续授权取消 Go 运行 oracle 准入，本次未运行 Go、未宣称 Go/Rust 运行差分。
既有 oracle-index.json 的运行项目不覆盖这些工厂；未借其记录充当本次证据。

只读核实 Go 的 Split/Merge/LeaveJoint/NonWitness 相邻测试及
`TestCreateLeaveJointStateOperatorWithoutFitRules`，再对照实际 Builder 源码：

- Split 保留三种策略及未知策略、重复和空的二进制 keys、范围与附加信息；
  incoming/demoting 均阻止 split。空 keys 不增加自创校验。
- 双向 merge 比较 membership，忽略 ID/顺序；不一致时先规划 source 副本变更。
  active/passive 元数据完整保留，配对 related-region 和 timeout，故障分配不返回
  部分操作；witness 差异不能被 membership 匹配忽略。
- scatter admin/non-admin 的 kind、force 语义与限流差异、随机目标、无 voter、
  无变化和轻量 add/remove。随机性是宿主显式索引，不声称复刻 Go map/rand 种子。
- leave-joint 保留合法 current/incoming leader，原 leader demoting 时先转移；
  普通候选失败后强制候选、仍无候选时仅 leave；跳过 placement fit，复用 leader
  负载评分。非 joint、无 leader 传播类型化错误。
- witness/non-witness 支持 voter/learner、缺失/重复状态错误、origin joint 拒绝，
  move-witness 复用批量转换。

42个固定场景：28成功、14预期错误；另有5项 membership matching 断言。
[测试](../../../pkg/schedule/operator/task_032_test.rs) 比较完整步骤 payload、kind、
epoch、ID、分配次数及输入所有权。成功操作逐步用真实 RegionInfo 回放心跳，
验证命令非空、pending/ack、ConfVerChanged、Influence、timeout、最终 peers/leader
及 Operator SUCCESS。active merge 按既有契约保持 pending，passive 的目标范围
改变后成功。单成员自动稳定与 witness 快照完成是显式外部心跳输入模型，未启动
真实 TiKV/Raft 集群。生产路径不涉及持久化，也没有新增 storage mock。

## 失败、修正与通过

[initial-compile.out](logs/task_032/initial-compile.out) 是最初误用 RegionMeta key
字段的编译失败，不计行为失败。[red.out](logs/task_032/red.out) 记录同一注册
`task_032_behavior_cases` 对 split 返回 PlanningRequired 而预期成功的断言失败：
退出101，1通过、1失败；补齐真实 split 实现后通过。

[green.out](logs/task_032/green.out) 保留两项源码审查后的固定期望修正：
Builder.SetLeader 第301行在无 voter 情况下先报 store 0 缺失，而非 NoVoter；
builder.go 537–544、651–660、906行表明 joint 的 non-witness voter 会走批量
switch，早先单条 NonWitness 测试片段不代表当前默认 joint 行为。
fixture 按这些只读源码分支修正，没有从 Rust 实现生成期望。
[replay.out](logs/task_032/replay.out) 保留新增最终状态字段接线的测试失败和最终
42场景通过；它不作为初始功能失败证据。

## 验证

全部 Cargo 命令由 [slot-run.py](logs/task_032/slot-run.py) 原子动态领取槽位，
实际槽位1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
脚本记录任务/计划/会话/PID，正常/失败/信号退出均等待子进程后释放自有锁，
保留缓存。确切命令、退出码和有效测试数量见
[validation.json](logs/task_032/validation.json) 及 validation-N.out。

workspace 测试复用既有 etcd 外部 I/O fixture，哈希和来源见
[provenance](logs/task_032/fixture-reuse-provenance.json)。
[只读验证入口](logs/task_032/readonly-validation.py) 使用 Go 拦截器，仅复制已存在
二进制，拒绝实际 Go 编译或其它命令；复制事件见 go-reuse.jsonl。
该二进制只是外部 I/O 后端，不是 operator oracle。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_032 -- --nocapture` | 0 | 2通过；42场景（28成功、14预期错误） |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 52通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1415通过、0失败、1既有忽略 |

[verification.json](logs/task_032/verification.json) 核对1119份受保护文件（1101份
Go/模块、18份 plan.md）哈希完全一致，无受保护 Git 状态变化、无新增 Go 文件；
3份 Go 来源与基准 commit 逐字节相同。`git diff --check`、文档相对链接检查通过。
fixture SHA256：`b95eefc47f27f2d9b5801077dd17365f11a32917a187015bbc0dde5381edbd70`。

已审查变更仅含本任务工厂、局部恢复规划、NotJoint 错误、lib/test 注册及对应
说明、fixture 和运行证据；没有修改既有普通/joint 规划流程、Go、Cargo配置或锁。
所有子进程已退出，自有槽位锁已释放，缓存保留。按 git-commit 技能仅提交任务32
实际变更；既有编号任务文件删除排除于提交，任务32完成后按授权删除。
