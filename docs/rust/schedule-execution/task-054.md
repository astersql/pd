# 任务054：拆分散射重试与回收

状态：已完成。

生产路径：[split_scatter_retry.rs](../../../pkg/schedule/checker/split_scatter_retry.rs)，
[API与宿主边界说明](../../../pkg/schedule/checker/split_scatter_retry.rs.md)。
`SplitScatterRetryController::{record_batch,top_pending,dispatch,cleanup_expired,clear}`
复用任务53 canonical observer；仅增加包内 snapshot/clear 接线，无新增 crate、依赖或 feature。

## 依赖与来源

前置批次53提交 `5b5c8ef85` 的观察/身份实现及
[验证证据](task-053.md) 已核实，其4个行为测试也纳入本任务包级验证。
整体和包级 plan.md 只读；读取 ADR-001、依赖基线、porting guide 和 oracle-index.json。
按用户明确后续授权，本任务证据为 **Go只读源码/测试契约推导 + Rust行为验证**。
没有执行 Go oracle，也没有声称运行差分。来源 commit、源码和 fixture SHA256 见
[source-provenance.json](logs/task_054/source-provenance.json)，Go运行输出及其哈希为 null。

只读 Go 测试对应关系：

- `TestSplitScatterControllerCleanupResetsPendingGauge`、`...ClearsPendingWhenDisabled`：清空和指标归零。
- `TestRecordSplitScatterBatchCollectsPendingRegions`：完整批次、共享group、pending gauge。
- `TestCheckSplitScatterRegionsCreatesScatterOperator`：真实operator准入及batch-group；散射算法是后继任务，宿主规划边界输出真实Operator。
- `TestDispatchSplitScatterKeepsPendingUntilSplitHeartbeat`、`...VersionLifecycle`：等待source、stale结果删除、新source不阻塞旧sibling。
- `TestCollectTopPendingDelaysMissingRegions`、`...BacksOffWhenInternalScatterOperatorExists`：10秒退避。
- `TestCollectTopPendingRemovesExpiredPending`、`...MarksAttemptedBeforeExpiration`：精确TTL和attempted分类。
- `TestRecordSplitScatterBatchRespectsPendingLimit`、`...ClearsExpiredPendingBeforeLimitCheck`：4096容量和先回收。
- `TestCollectTopPendingSortsBeforeLimit`、`...PrioritizesNearExpiration`：deadline/group/ID稳定排序。
- `TestDispatchSplitScatterBacksOff`、`...BacksOffWhenHot`、`...RespectsScheduleDeny`：重试原因、失败与真实标签守卫。
- `TestDispatchSplitScatterRespectsScheduleLimit`、`...CleansExpiredPendingBeforeEarlyReturn`：运行数量门限、先过期再早退。

## 测试与故障

[冻结fixture](fixtures/task_054.json)按只读Go源码独立写定5个TopPending/TTL表驱动输入期望，
不从待测实现生成。时钟为宿主传入单调Duration秒；10秒退避和30分钟TTL均比较精确边界，
不归一化忽略group、状态、错误或operator差异。

[task_054_test.rs](../../../pkg/schedule/checker/task_054_test.rs)显式注册6个真实行为测试：

1. `task_054_behavior_cases`：5个固定表项（全部就绪、limit1、limit0、TTL前、精确到期）。
2. `task_054_missing_backoff_expiration_and_stale_identity`：result/source缺失、9/10秒重试、attempted/unattempted过期各1、旧身份不得更新、旧版本等待与新版本删除。
3. `task_054_order_capacity_and_running_operator`：deadline先于group/ID、同deadline排序、真实运行operator延迟、4096容量原子拒绝、覆盖与过期回收、disabled/空批次。
4. `task_054_dispatch_outcomes`：7个表驱动派发结果（hot、balancedCPU、其它retry、规划错误、无需operator、实际operator、实际准入拒绝），另有真实副本守卫、disabled、全局上限、先回收、空候选退避和取消。
5. `task_054_real_labels_store_budget_and_sequential_admission`：真实RegionLabeler deny、实际存储令牌消耗后拒绝、后项观察前项运行delta、可用容量仅选1且未选中项保持unattempted。
6. `task_054_heartbeat_and_range_group`：source心跳等待、全局9/10秒门限、新source允许旧sibling、真实codec范围分组、真实operator保留group与batch-group。

测试使用真实core、placement manager、labeler、store limiter、operator和admission；仅时钟及
宿主规划边界可控。RetryLater/规划错误在宿主边界注入，断言实际pending/deadline/错误/指标。
无本任务持久化行为，不创建MetaStore假实现。labeler/placement的真实初始化使用已有MemoryStore。

先写并注册测试、临时未实现TTL的控制器，[red.log](logs/task_054/red.log)退出101，
同一个行为测试在 `ttl-inclusive` 精确边界断言 `[100,101] != []`，非缺包/缺注册失败。
实现后同一测试通过；新增真实组件用例期间修正测试输入：ProviderSnapshot默认副本数为0，
必须显式设3才能证明未充分复制守卫；存储成本来自region统计，必须提供真实size/keys。


[mutation.py](logs/task_054/mutation.py)依次将TTL改为严格大于、取消退避、取消attempted标记、
反转expiry排序。四次均退出101且触发实际行为断言，见
[mutation-results.json](logs/task_054/mutation-results.json)。脚本finally恢复原生产源码，
随后重新格式化并运行最终验证。正向派发fixture已加强为3个真实voter及leader，
未充分复制用例要求4副本而实际只有3副本；不依赖0副本配置证明正向调度。

## 共享槽位与验证命令

[run.py](logs/task_054/run.py)对每个Cargo阶段原子mkdir动态领取仓库槽位1–10，
记录计划/任务/会话/PID。子进程继承绝对CARGO_TARGET_DIR；异常/中断会终止并等待
子进程组，再移除自己的owner记录与锁目录。保留Rust编译缓存。
所有已执行阶段实际领取 **槽位1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
命令、退出码和环境均记录在同名JSON，完整stdout/stderr在同名log。

首轮 `RUST_TEST_THREADS=2 cargo test --workspace --locked` 退出0，
1497通过、0失败、1既有忽略（含2个compile-fail文档测试）。
见[首轮完整日志](logs/task_054/workspace-test.log)和
[命令记录](logs/task_054/workspace-test.json)。
验证期间磁盘空间下降；仅在持有槽位1期间清理其中超过6小时且持有PID不存在、
进程命令未引用的22个旧etcd数据目录，约1.745GB。未清理Rust缓存、其它槽位或活跃锁；
[清理记录](logs/task_054/fixture-cleanup.json)逐项保留路径、PID和字节数。

## 差异审查

[audit.json](logs/task_054/audit.json)逐一核对1101个Go/模块文件的清单和SHA256，
与任务开始一致，Go Git状态仍为空；两个只读plan.md内容与HEAD相同。
文档相对链接有效，`git diff --check`通过。任务无Go生成、格式化、make或failpoint写入。
workspace既有测试中构建的只读辅助fixture不作为本任务Go运行oracle。
提交仅包含本任务模块/局部接线、测试、fixture、生产说明和证据，排除既有其它任务删除。
日志只去除行末空白和末尾多余空行，未改写错误、状态或测试输出。


## 最终结果

变异恢复并加强3副本fixture后，下列最终命令均退出0：

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_054 -- --nocapture` | 0 | 6通过，0失败，0忽略 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 39通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1497通过，0失败，1忽略 |

最终完整workspace为 **1497通过、0失败、1既有忽略**（1495个运行测试、2个compile-fail文档测试）。
既有忽略为 `tso_processes`，未新增忽略。聚焦6个测试包含5个冻结TopPending/TTL表项、
7个派发结果表项，以及前述边界与故障场景，均0失败。
见[最终日志](logs/task_054/workspace-test-final.log)、[测试计数](logs/task_054/test-summary.json)
和[全部Cargo命令/退出码/槽位](logs/task_054/commands.json)。
全部Cargo及测试子进程已退出，自己的槽位锁已释放，Rust缓存保留。
