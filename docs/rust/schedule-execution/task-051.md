# 任务051：优先区域巡检

状态：已完成。

生产实现：[priority_inspector.rs](../../../pkg/schedule/checker/priority_inspector.rs)，
边界说明：[priority_inspector.rs.md](../../../pkg/schedule/checker/priority_inspector.rs.md)。
公开 API 为 `PriorityInspector::new/inspect/get_priority_regions/remove_priority_region`，
以及只读 `len/is_empty/entry`。`inspect` 返回真实 placement `Arc<RegionFit>` 或
replica 模式的 None；时间和配置由宿主提供，不创建定时器、运行时或 operator。

## 依赖与来源

批次50由提交 `a95fac07e`、[任务50证据](task-050.md)、真实 affinity/placement/
checker API 及本次包级回归确认。读取了整体与包级只读 plan.md、oracle-index.json、
ADR-001、依赖基线及移植指南。没有改写这些指南。oracle-index 的任务51条目
均为 source_assertion，runtime_cases 没有优先巡检输出；按用户授权采用下述源码依据。

按用户后续授权，本任务以**只读 Go 源码与测试契约推导加 Rust 实际验证**验收，
没有运行 Go checker oracle，没有声称运行差分。来源、commit、fixture SHA256
见 [source-provenance.json](logs/task_051/source-provenance.json)；Go 输出哈希为 null。

`priority_inspector.go` 的 Inspect/两个模式检查/addOrRemoveRegion/GetPriorityRegions/
RemovePriorityRegion，`priority_queue.go` 的 Put/Remove/Elems/Entry.Less，以及
`placement/fit.go` 的 RuleFit.Peers/PeersWithDifferentRole 是冻结期望的只读来源。
相邻 `TestCheckPriorityRegions` 的源码直接覆盖两种模式下健康 region、缺1/2副本、
严重缺口先运行、修复后移除、重复检查后延迟和手动移除。它没有直接覆盖 epoch、
相同优先度、队列容量、learner 规则或精确重试时间边界；这些从上述生产源码推导。
没有将测试名称或 Rust 实际结果伪称为 Go 运行输出。

## 行为与故障证据

[task_051_test.rs](../../../pkg/schedule/checker/task_051_test.rs) 显式注册两个测试：
`task_051_behavior_cases` 和 `task_051_concurrent_removal`。固定
[fixture](fixtures/task_051.json) 共10组场景、58个动作案例。用真实 RegionInfo、
StoreInfo、ProviderSnapshot、RuleManager、MemoryStore/StorageEndpoint 与 canonical
PriorityQueue；没有 mock placement 或从待测实现生成期望。

覆盖 replica/placement、正常/缺失/过量副本、返回 fit、优先度顺序、epoch 改变、
attempt/last 变化、严格截止时间（相等不返回）、优先度变化保留旧重试状态、恢复移除、
手工移除及不存在ID、并列替换/隐藏ID再入队/删除并列ID、容量0/1/2拒绝与抢占、
replica 模式计入 learner、placement 跳过 learner 规则缺口、已分配角色不匹配仍计数、
跨规则边界空fit。并发测试以8个线程各100次移除/快照验证最终状态与幂等性。
本行为不做持久化写入，也没有可传播的 I/O 错误；真实规则初始化/设置均检查返回值。

Go 的 priority-only BTree 相等关系会让并列 region 只保留一个排序项、多个 map 项；
删除隐藏的 map ID 可删除该优先度当前排序项。本任务复用已有 canonical 队列，
如实保留这项行为。改变 priority 时 Put 保留旧 Value，所以 attempt/last 不重置。
角色不匹配 peer 仍存在于 RuleFit.Peers，不能以 RuleFit 是否 fully satisfied 代替缺口计数。

[red.log](logs/task_051/red.log) 为初次 API 类型不匹配的编译失败，不计行为证据。
测试配置显式设 max_replicas=3、有效 voter 规则，并从 Go RuleFit 契约校正角色边界后，
[red-behavior.log](logs/task_051/red-behavior.log) 记录漏入队导致断言失败的真实负例。
同一注册测试在恢复实现后通过。

[mutation.py](logs/task_051/mutation.py) 按阶段领取槽位、先 fmt、运行测试并 finally
恢复源文件。[mutation-results.json](logs/task_051/mutation-results.json) 中五项
错误均 exit101 且有行为断言失败：漏入队、把严格截止比较改为相等也返回、计入 learner
规则缺口、漏移除恢复region、attempt固定为1。全量验证在全部变异恢复后运行。

## 验证

[run.py](logs/task_051/run.py) 每次依次原子 mkdir 尝试共享槽位1–10，记录计划、任务、
会话与PID；所有 Cargo 子进程继承绝对 CARGO_TARGET_DIR。正常/失败/中断时等待
子进程组退出后清理自己的锁，保留缓存。实际命令和退出码见
[commands.json](logs/task_051/commands.json)，最终回归使用 `RUST_TEST_THREADS=2`。

所有阶段实际槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_051 -- --nocapture` | 0 | 2通过、0失败、0忽略；10组/58动作案例 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 27通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1485通过、0失败、1忽略 |

既有忽略为 `tso_processes`，没有新增忽略。日志仅清理行末空白和末尾空行。
全部子进程已退出，自己的槽位锁已释放，编译缓存保留。

新增 checker 对已有 workspace cache crate 的依赖，Cargo.lock 仅多出一条本地依赖。
离线生成锁后排除了不相关远程包更新，没有远程依赖版本变更或新增 feature。

## 差异审查

Go 与模块文件全程只读；未运行 Go 测试、make、failpoint 注入或格式化。
workspace 既有测试可能只读构建 Go helper 到领取槽位，它不构成 checker oracle。
最终 [audit.json](logs/task_051/audit.json) 核对全部 Go/模块内容及Git状态、只读
plan哈希、自己的槽位释放和 `git diff --check`。仅提交本任务生产代码、测试、
Cargo接线、说明及证据；保留并排除工作区中其他编号任务文件的已有删除。
