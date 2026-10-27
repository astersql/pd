# 任务120：合并与拆分管理操作

批次119真实依赖已满足：提交 `4980f123b`、[119证据](task-119.md)与已验证
Handler/Coordinator/RangeOperatorFactory/ControllerAdmission/records 接口均已核对。
整体及包级 plan.md 只读。用户后续明确授权以只读 Go 源码/测试推导及 Rust 真实
行为和故障验证替代缺失 Go 运行 oracle；这里没有 Go 运行差分的声明。

生产路径：[admin_range.rs](../../../pkg/schedule/handler/admin_range.rs)、
[说明](../../../pkg/schedule/handler/admin_range.rs.md)。公开 API 为
`Handler::add_merge_region_operator`、`Handler::add_split_region_operator`、
`AdminRangeContext`、`AdminRangeError`。复用真实复制健康过滤、RuleManager 拟合、
范围工厂、原子接纳与替换归档；无 server/proto 反向依赖或元数据写入。
Cargo 仅新增已有 filter 路径依赖，将已有 hbstream 从测试依赖提升为生产依赖；
Cargo.lock 只新增该 filter 依赖边，无远程依赖版本变化。

[provenance](logs/task_120/provenance.json)冻结源码 commit、当前 checkout commit、
Go 源文件及 [fixture](fixtures/task_120.json) SHA-256。Go 运行命令、二进制和输出均
为 null。已核对 oracle-index 已有运行案例不覆盖本故事，未复用不相关运行结果。
只读依据为 `handler.go:581–659`、`filter/healthy.go`、
`operator/create_operator.go:154–235` 和
`tests/server/api/operator_test.go::checkMergeRegionOperator`。源码与基准 commit
的一致性逐文件记录；测试源码只作推导依据，未运行需 failpoint 的 Go 测试。

fixture 固定五项 split 的策略名、输入 hex 和期望原始 policy/二进制 keys；
合并输入为三段 `[a,b)`, `[b,c)`, `[c,+∞)`，分别携带 `(1,2)`、`(2,3)`、`(3,4)`
epoch 与三个 voter，四个方向的正常 pair 产生 active/passive 操作。
额外错误/故障输入与预期冻结在独立测试中的字面表格，未从 Rust 实现生成期望。
时钟和能力由宿主显式传入，不忽略错误、步骤、epoch 或操作目标。

独立注册6项真实行为测试，共48个场景，15个成功及33个预期错误：

- `task_120_behavior_cases`：5项 split 与4项有向 merge；断言 policy、keys、范围、
  epoch、ADMIN/URGENT、STARTED、active/passive、双向关联 ID 和实际命令。
- `task_120_validation_and_atomicity`：15项校验/冲突错误，包括 source/target 缺失、
  首尾空边界伪相邻、同 region、无效 policy、奇数 hex、非 hex/Unicode、有效键后
  无效键、缺 region 先于 policy、merge 中 target 已有 URGENT 操作、重复 split。
  拒绝后不入队，已有操作保留同一 Arc，双操作不发生部分接纳。
- `task_120_health_joint_and_allocation_fault`：source/target 分别覆盖 down/pending、
  副本不足/过多、learner 与联合状态，共12项；split 联合状态拒绝、真实 allocator
  I/O 故障及恢复后成功，共15项。恢复后逐项验证 add(4,100)/promote/remove(3)/merge。
- `task_120_placement_and_epoch`：真实 RuleManager 未初始化时拒绝、通过 MemoryStore
  初始化3副本默认规则后成功，以及真实 admission 对 stale target epoch 原子拒绝，
  共3项。epoch 故障直接施加于现有 factory/admission 边界，不虚构 Handler 并发接口。
- `task_120_split_preserves_source_validation_boundary`：down/pending、少副本、无 leader
  四项 split 仍成功，源 RegionInfo Arc 不变；保留 Go split 的校验范围。
- `task_120_unbootstrapped`：两项操作在无 Coordinator 时优先报告未初始化。

harness 使用真实 BasicCluster、Coordinator、RangeOperatorFactory、ControllerAdmission、
MemoryStore 和 RuleManager，未用 mock 调用次数代替状态。仅宿主时钟/能力与 allocator
I/O 边界可控制。管理动作没有持久化写入，故无虚设的存储失败开关。

失败证据：[initial](logs/task_120/initial.json)记录缺失模块/API 的编译错误 exit101，
不是行为验收；[first](logs/task_120/first.json)是测试装配遗漏 max_replicas=3 导致的
AbnormalPeer，修正 harness 后通过，同样不充当生产行为红灯。
[behavior-red](logs/task_120/behavior-red.json)将 SCAN 错误映射为 APPROXIMATE，
同一真实行为测试实际断言 `left:1, right:0`，exit101，4通过/1失败；恢复正确映射后
同一断言通过。没有只保留缺注册/缺 API 失败。

最终 [final.json](logs/task_120/final.json) 保存7条确切命令、退出码、有效测试数、
原始 stdout/stderr 的压缩日志及 SHA-256。所有必需命令均 exit0：

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-handler --locked --lib task_120 -- --nocapture` | 0 | 6通过、0失败、0忽略；48场景 |
| `cargo test -p astersql-pd-schedule-handler --locked` | 0 | 15通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace准入 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | 全workspace编译检查 |
| `cargo test --workspace --locked` | 0 | 1787通过、0失败、1项既有ignored；含2项compile-fail文档测试 |

所有阶段均通过原子 mkdir 动态领取槽位1，绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话/PID；全部Cargo和槽位子进程结束后已移除自己的owner/锁，
缓存保留。环境 `CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 GOFLAGS=-mod=readonly`。
workspace既有etcd fixture只编译已有Go工具到槽位；proto生成入口只读模块ZIP，
未运行failpoint启停、Go格式化或模块整理命令。已有server弃用字段warning不影响通过。

[最终只读复核](logs/task_120/verification.json)确认1101份Go/模块文件路径和内容哈希
与开始时逐一完全一致，Git无Go/模块差异，两份plan.md哈希不变；
[压缩起始清单](logs/task_120/readonly-start.json.gz)保留逐文件哈希及已有Git状态。
`git diff --check` exit0，文档相对链接均有效。已审查范围：仅admin_range、独立测试、
Handler模块注册、必要Cargo依赖边和120证据；保留其它编号任务的已有删除，不纳入提交。

状态：已完成。
