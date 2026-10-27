# 任务121：操作创建输入分派

依赖120已核实：提交 `82260f2f2` 和 [任务120证据](task-120.md)的48项场景、
6项聚焦测试、15项包测试、1787项 workspace 测试以及 canonical Handler/range
API 均已核对。两份 plan.md 保持只读。

用户后续明确允许以只读 Go 源码/测试推导与 Rust 真实行为/故障验证替代缺失的
Go 运行 oracle。oracle-index 中 HandleOperatorCreation 仅有 source_assertion，
既有运行输出不覆盖本故事。本任务没有执行 Go 行为 oracle，也没有声明运行差分。
[provenance](logs/task_121/provenance.json)保存基准、checkout commit、源码/fixture
哈希及逐文件基准一致性；Go binary/command/outputs 均为 null。

生产 API：[admin_parse.rs](../../../pkg/schedule/handler/admin_parse.rs)的
`OperatorCreation::parse`、`handle_operator_creation`、`CreationContext`、
`CreationResponse`、`CreationError`；[边界说明](../../../pkg/schedule/handler/admin_parse.rs.md)。
输入覆盖十种 action，执行复用真实119/120 API和 Coordinator 的散射状态。
可变 Coordinator 借用只用于 canonical scatter 分布计数；不复制状态或算法。
局部公开已有 ScatterBatch::scatter_one，单区散射保留 HIGH、带 store-limit 接纳，
suspect 转发 checker，替换归档复用现有 records。批量失败保留200的百分比/error body。
无新的持久化写入；无 mock 管理回调或伪造 MetaStore 故障。

[fixture](fixtures/task_121.json)冻结32项输入、期望状态、错误、描述或响应体；
额外测试期望固定在独立 task_121_test.rs 的字面表格，与实现分离。共4项真实测试、
63个场景（不存在忽略项）：

- `task_121_behavior_cases`：32项；全部10种成功动作，17项类型/缺字段错误、
  3项非法数字ID、缺区域500以及批量缺区域200。逐步验证目标store/peer ID、
  transfer/promote/remove、split policy、merge target、ADMIN/URGENT及STARTED。
- `task_121_roles_and_numeric_boundaries`：15项；5种roles类型/长度与未知角色、
  重复store的非字符串不覆盖、4种非法输入、0及2^53精度边界、3种retry输入。
- `task_121_faults_and_response_shapes`：10项；真实allocator I/O失败、恢复后
  实际AddLearner(4,501)、单区热区错误、批量失败0%、奇数/ASCII非法字节/Unicode非法字节hex范围、3种未初始化
  优先级/响应形状；拒绝后控制器无操作。
- `task_121_scatter_accounting_and_real_commands`：6项；批量50%部分成功、范围
  优先于错误ID、同group三次单区散射验证真实leader(1→2)、HIGH及重复请求成功，
  未充分复制拒绝并将ID加入真实checker pending_processed。

测试使用真实 BasicCluster、Coordinator、RuleManager、MemoryStore、工厂、
ControllerAdmission、散射计数及 checker。仅 allocator I/O、时钟、热区域与可用store
快照由宿主控制。默认规则通过 MemoryStore 真正初始化。Go map/tie 按既有 Rust
实现使用升序确定性顺序，不声称和 Go 随机目标逐字节相同。

非法浮点 ID 按任务要求返回400，非数组keys返回400替代 Go panic；这些明确的输入
边界不伪称 Go 运行结果。JSON数字经float64；retry在int64范围内截断，执行上限30，
负值无尝试。其余字段与错误不被归一化或省略。

失败证据：[initial](logs/task_121/initial.json)是缺模块/API编译失败，不能作为行为红灯。
[first](logs/task_121/first.json)暴露测试harness缺少默认规则；expanded系列保留
新增测试编译/装配及断言调整的实际失败，不作为实现正确的证明。
[behavior-red](logs/task_121/behavior-red.json)将add-peer故意错派为add-learner，
同一冻结fixture实际报告 `admin-add-learner != admin-add-peer`，exit101、3通过/1失败。
已恢复正确分派。最终源码复核还修正了单区scatter的HIGH及store-limit接纳，
不保留早期误认为全部管理动作URGENT的假设。

早期 `final.json` 只记录修正前预检；为使全量验证覆盖最终源码，其 workspace
运行已主动中断（退出130），等待该进程组结束并释放槽位后重新领取。
只以 `final-corrected.json` 作为最终验收，未把中断或旧源码测试当作通过证据。

最终 [final-corrected.json](logs/task_121/final-corrected.json) 保存确切命令、退出码、
压缩原始 stdout/stderr、日志 SHA-256 和有效测试计数。全部必需命令退出0：

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-handler --locked --lib task_121 -- --nocapture` | 0 | 4通过、0失败、0忽略；63场景 |
| `cargo test -p astersql-pd-schedule-handler --locked` | 0 | 19通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace准入 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | 全workspace检查 |
| `cargo test --workspace --locked` | 0 | 1791通过、0失败、1项既有ignored；含2项compile-fail文档测试 |

各阶段原子mkdir动态领取槽位1；绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话/PID；Cargo及使用槽位的子进程结束后已释放自己的锁，保留缓存。
[verify.py](logs/task_121/verify.py)保存可重复执行及中断后等待子进程退出的清理逻辑。
`CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 GOFLAGS=-mod=readonly`。
既有etcd测试仅把只读Go fixture编译到槽位；proto构建只读固定模块ZIP，所有子Cargo
继承同一目标路径。没有执行failpoint启停、Go格式化、生成或模块整理入口。
既有server deprecated字段warning不影响通过。

[最终复核](logs/task_121/verification.json)确认1101份Go/模块文件逐一哈希和路径
与[起始清单](logs/task_121/readonly-start.json.gz)完全一致，Git无Go/模块差异，
两份plan.md哈希不变。15个相对文档链接有效，`git diff --check`退出0。
审查范围仅包含admin_parse、独立测试、模块/依赖提升、公开已有scatter规划方法及
说明和121证据；Cargo.lock未变化，无远程依赖版本变化。保留其它任务既有删除，
不纳入本任务提交。

状态：已完成。
