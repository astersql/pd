# 任务056：区域巡逻工作生命周期

状态：已完成。

生产路径：[patrol.rs](../../../pkg/schedule/checker/patrol.rs)、
[宿主边界与使用说明](../../../pkg/schedule/checker/patrol.rs.md)。
公开 API：`Patrol::{new,subscribe,run}`、`PatrolHost`、`PatrolConfig`、`PatrolStatus`、
`PatrolSource`、`PatrolError<E>`、`patrol_scan_limit`，以及
`CheckerDispatcher::check_patrol_region`。

## 依赖与来源性质

批次55的提交 `5afa1bfce`、[依赖证据](task-055.md)、真实 checker dispatcher、
priority inspector、pending TTL 和 admission API 已核实；44个前置checker测试重新通过。
已读取只读整体/包级 plan.md、ADR-001、依赖基线、porting guide、oracle-index.json。

遵循用户后续授权，以 **只读 Go 源码/测试契约推导 + Rust 行为验证** 实现。
未运行本任务 Go 调度 oracle，未声称运行差分；Go 输出及输出哈希均为 null。
workspace 既有测试会只读构建/启动 etcd fixture，不能据此声称取得调度 Go oracle。
[source-provenance.json](logs/task_056/source-provenance.json)记录来源 commit、Go 文件 SHA256、
fixture SHA256 与依赖；[fixture](fixtures/task_056.json)冻结7个limit输入/期望和10个生命周期故事。
不对错误、操作、目标或状态作归一化；使用宿主暂停时钟检查精确deadline和duration。

Go来源为 `checker_controller.go`：

- `calculateScanLimit`：count除以1024后截断、限制到128–8192；channel容量8192。
- `PatrolRegions`：首tick启动配置workers；动态tick/worker调整；halt清空backlog；
  backlog不空则跳过；priority → pending → split-scatter → scan → labels。
- `checkRegions`：最后end key为cursor；空scan重置cursor但不刷新duration/limit；
  非空且end key为空才完成轮次并更新limit。
- `checkPriorityRegions`：删除缺失ID，跳过merge pair，不额外修改pending；
  `tryAddOperators` 的pending/scan路径复用批次55。
- `updatePatrolWorkersIfNeeded`、`PatrolRegionContext::{stop,startPatrolRegionWorkers}`：
  取消idle workers，等待在途检查完成；更换workers前等待旧一代；停止等待完毕后清理split-scatter。

## 实现与行为验证

生命周期在宿主task/LocalSet内并发轮询worker futures，不建立私有runtime或独立状态副本。
`PatrolHost`桥接宿主持有的registry、checker、split-scatter与统计；
`check_patrol_region`直接调用真实dispatcher/admission，保留priority与pending不同的状态转移。
停止方式为cancel token后await run；直接drop future属于非优雅中断。
零interval和宿主检查故障返回类型化错误，错误退出仍等待全部workers并清理split-scatter。

测试使用真实 BasicCluster/RegionsInfo、CheckerDispatcher/PriorityInspector、TTLUint64、
RuleManager、affinity Manager、MemoryStore、ControllerAdmission、LabelStatistics、
SplitScatterRetryController。只控制宿主时钟、外部检查阻塞/故障和聚合region count快照。
8192容量测试的本地registry含8192个真实region，聚合count输入8388608使scan limit为8192；
不构造数百万重复region来代替聚合统计输入。

[task_056_test.rs](../../../pkg/schedule/checker/task_056_test.rs)的8个有效测试：

1. `task_056_behavior_cases`：7个冻结limit边界，包含截断与上下限。
2. `task_056_real_priority_pending_and_scan_order`：真实priority按缺失数量排序、缺失ID删除，
   pending缺失ID保留/有操作移除；真实promote-learner步骤与等待队列；阶段顺序与label统计结果。
3. `task_056_cursor_backlog_dynamic_tick_and_workers`：128+2+empty扫描，cursor129/131/empty，
   backlog不推进；workers 0→2；tick 10→25ms并重设deadline。
4. `task_056_shutdown_and_resize_wait_for_inflight`：取消和2→1两种序列，阻塞两个在途检查，
   确认释放它们前不清理/不启动新一代，释放后完成真实检查并等待全部worker退出。
5. `task_056_saturation_halt_and_cancel`：0 worker填满8192 channel，backlog跳过、halt清空、恢复扫描，
   满channel停止也能结束。
6. `task_056_worker_fault_and_invalid_config`：类型化worker故障、初始和动态0interval，检查错误与清理状态。
7. `task_056_round_boundary_updates_limit_and_duration`：count130→132096，只有非空终点扫描刷新128→129，
   精确30ms轮次耗时，取消归零。
8. `task_056_priority_merge_and_pending_admission_contracts`：先证明真实planner生成merge pair，
   priority跳过但scan实际入队两个操作；真实queue rejection下priority保留pending，普通重检移除pending。

## 失败证据与回归识别

先注册真实测试并用返回0的最小实现运行：
[red-behavior.log](logs/task_056/red-behavior.log)退出101，`left:0/right:128`。
最初[red.log](logs/task_056/red.log)为需要更新lockfile的错误，不计为行为失败证据。
`cargo generate-lockfile --offline`之后恢复无关pulldown-cmark升级，最终lock只新增futures-util依赖边。
中途的编译和priority测试失败保存在focused/focused2日志；priority测试的两个相同缺失优先级
触发已有Go兼容queue的tie替换，改为不同缺失数量后验证实际排序，并未改动依赖queue实现。

[mutation.py](logs/task_056/mutation.py)依次跳过priority、清空cursor、跳过最终worker等待。
三次均退出101并命中行为断言，详见[结果](logs/task_056/mutation-results.json)及同名log；
finally恢复生产文件，之后重新格式化并完成最终验证。
priority变异测试先断言阶段数充足，避免前缀切片越界被误算为行为断言。

## Cargo共享槽位与最终验证

[run.py](logs/task_056/run.py)每个验证阶段原子mkdir动态领取1–10槽位，记录任务、会话、PID；
所有Cargo和子进程继承绝对CARGO_TARGET_DIR。错误/中断终止并等待子进程后释放自己的锁，保留缓存。
[validate.py](logs/task_056/validate.py)按顺序运行最终验证。
所有阶段实际槽位为1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

以下命令全部退出0。逐条命令、环境和退出码见[commands.json](logs/task_056/commands.json)。

| 命令 | 有效结果 |
| --- | --- |
| `cargo fmt --all` | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_056 -- --nocapture` | 8通过，0失败；7个limit表项、10个生命周期故事 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 52通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过；仅既有server弃用字段告警 |
| `RUST_TEST_THREADS=2 cargo test --workspace --locked` | 1510通过，0失败，1既有忽略 |

workspace计数含1508个运行测试和2个compile-fail文档测试；既有忽略为`tso_processes`。
[完整workspace日志](logs/task_056/workspace-test.log)、[包级日志](logs/task_056/package-final.log)、
[最终聚焦日志](logs/task_056/focused-provenance.log)及[计数](logs/task_056/test-summary.json)可独立审查。
补存fixture来源/场景说明后又运行一次聚焦测试，8个测试通过；未更改生产逻辑。
日志仅清理行末空白及末尾多余空行，没有改写状态、操作、错误或测试输出。

[audit.py](logs/task_056/audit.py)与[结果](logs/task_056/audit.json)核对1101个Go/模块文件清单与
SHA256完全一致，Go Git状态为空；两份只读plan.md与HEAD一致，文档链接有效，差异检查通过。
审查范围限于新增patrol实现/说明/独立测试、lib注册、必要依赖位置调整和futures-util依赖边、
fixture与本任务证据；未包含原有其它任务文件删除。
全部Cargo及测试子进程已结束，自己的槽位1锁已释放，编译缓存保留。
