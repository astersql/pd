# 任务058：检查器观测指标

状态：已完成，待回归。聚焦行为及包级验证通过；workspace test 因磁盘耗尽未通过，
保留编号任务文件供环境恢复后回归，不满足后继完整回归门槛。

生产路径：[checker_metrics.rs](../../../pkg/schedule/checker/checker_metrics.rs)，
[边界说明](../../../pkg/schedule/checker/checker_metrics.rs.md)。
API：`CheckerMetrics::{register,event,observe_checker_metrics}`、`CheckerMetricKind`，
`CheckerDispatcher::set_metrics`、`Patrol::set_metrics`、`SplitScatterRetryController::set_metrics`。
复用现有 canonical cluster、placement、affinity、MemoryStore、operator admission、
pending TTL、patrol worker 和 split-scatter retry 状态；未增加模拟状态面或 Go 实现。

已核实批次57提交 `2996f7078` 和[证据](task-057.md)、SuspectRanges/pending/patrol API。
整体及包级 plan.md、oracle-index.json、ADR-001、依赖基线和移植指南均已阅读且只读。
按用户后续授权采用**只读 Go 源码/测试契约推导 + Rust 行为验证**，没有运行 Go oracle，
没有宣称运行差分，Go输出及其哈希均为 null。
[provenance](logs/task_058/provenance.json)保存真实来源提交、源码和 fixture SHA256；
[fixture](fixtures/task_058.json)冻结9个 family 的 name/help/type/labels、113个事件标签、
6个phase、11个默认桶和25ms显式耗时输入。

宿主独立 Registry，预缓存 label 句柄并通过 Arc 共享；未知事件报错，不新增高基数标签。
注册第一个/最后一个 collector 碰撞时回滚本次注册，保留已有 collector。
巡逻队列 gauge 采样时点和 Go 保持一致；shutdown只归零轮次耗时和 split-scatter pending，
队列 gauge 保留最后采样。各 checker duration 随实际进入阶段更新，首个非空结果后不计后续阶段。
具体事件接线范围见边界说明；事件词表覆盖不等同于113条业务分支的运行覆盖。

[独立测试](../../../pkg/schedule/checker/task_058_test.rs)共8个有效测试：

1. `task_058_behavior_cases`：真实 Counter 累加、25ms histogram count/sum。
2. `task_058_schema_and_bounded_events`：9个 family 的 name/help/type/label，113个事件逐项递增，
   未知事件拒绝且 series 数不变，6个phase与11个桶逐项一致。
3. `task_058_registry_isolation_and_collision_rollback`：首/末注册碰撞、回滚后的再次注册、
   clone共享、两个Registry独立，计数分别为2和1。
4. `task_058_real_dispatch_and_patrol_lifecycle`：正常/暂停2个故事，以真实领域Host巡逻、
   learner admission、dispatcher duration/event、halt、cancel/join和清理结果验收。
5. `task_058_split_scatter_transitions`：新增、重复覆盖、重复TTL回收、4096容量拒绝、
   clear归零且Counter保留；过期未尝试2、丢弃2。
6. `task_058_host_failure_and_pre_cancel`：预取消/宿主I/O错误2个故事，不伪造checker样本，
   原错误保持，最终清理。
7. `task_058_queue_snapshots_and_backlog`：priority/pending快照各2、channel backlog2，
   后续只记录wait phase；暂停learner事件2，限额跳过merge仍记录duration2但check事件0。
   输入使用不同副本缺口，尊重canonical priority queue的priority-only同值等价规则。
8. `task_058_split_missing_event_and_attempted_expiration`：真实registry缺失结果事件2、
   10秒退避、只选择1项、到期attempted/unattempted各1。

失败—通过证据：

- [首次缺API](logs/task_058/red-api.log)：退出101，未定义CheckerMetrics/CheckerMetricKind；
  初始测试还含Prometheus proto accessor命名错误，已修正，没有把该编译失败当成行为证明。
- [行为变异](logs/task_058/red-behavior.log)：临时将真实event增量改为0，编译成功，
  同一组8测试中6失败、2通过，断言明确得到0而非1/2。随后恢复实现。
- [最终聚焦](logs/task_058/focused.log)：退出0，8通过、0失败、0忽略。
- [包级](logs/task_058/package.log)：退出0，66通过、0失败、0忽略；doc-tests为0，不计行为数。

每个阶段均用 [run.py](logs/task_058/run.py) 从仓库根依次原子mkdir动态领取共享槽位，
实际均获得槽位 **1**，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划、任务、会话和PID，子进程退出后finally释放自己的锁；有效缓存保留。
每次Rust编辑后先执行 `cargo fmt --all`。确切命令/退出码另存同目录JSON：

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_058 -- --nocapture` | 0 | 8测试通过 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 66测试通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace可解析 |
| `cargo fmt --all -- --check` | 0 | 格式通过 |
| `cargo check --workspace --locked` | 0 | workspace编译检查通过 |
| `cargo test --workspace --locked` | 101 | rustc磁盘ENOSPC，未形成有效全仓行为测试结果 |

[workspace失败日志](logs/task_058/workspace-test.log)记录rustc输出流、fingerprint和临时目录
均报磁盘空间耗尽。Cargo结束后连JSON写入也报ENOSPC，锁仍由finally释放；
[恢复记录](logs/task_058/workspace-test.json)如实标明退出码的日志恢复来源。
已尝试清理本任务的中间日志/临时生成器和失败编译的77个未完成rcgu.o（8,398,472字节），
恢复到约122MiB可用，保留有效缓存和其他任务缓存；仍不足支撑全仓重新编译。
不执行全局cargo clean或清理用户文件。空间恢复后，重新动态领槽运行上述workspace test，
通过后更新为已完成并删除编号任务文件。

Cargo.lock只增加本crate的既有prometheus依赖，未改变远程依赖版本。
[审计脚本](logs/task_058/audit.py)与[结果](logs/task_058/audit.json)核对全部1101个Go/模块文件
清单及SHA256不变、Go Git状态为空、两份只读plan.md不变、链接有效、`git diff --check`通过。
提交排除任务开始前已有的其它编号任务删除，保留本任务的待回归状态文件。

日志仅清理行末空白及末尾空行，未改写错误、测试、指标或状态输出。
