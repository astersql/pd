# 任务059：单区域散射目标选择

状态：已完成。实现、聚焦/包级及全仓基线全部通过。

生产路径：[scatter_target.rs](../../../pkg/schedule/scatter/scatter_target.rs)，
[边界说明](../../../pkg/schedule/scatter/scatter_target.rs.md)。公开API：
`ScatterTargetSelector::select`、`ScatterCounts::{set,get}`、`ScatterTarget`、
`ScatterTargetError`。新增 canonical crate `astersql-pd-schedule-scatter`，
只注册必要依赖；Cargo.lock 仅增加本地crate，没有远程版本升级。

已核实前置58提交 `60b4b588e`、[证据](task-058.md)及其8个聚焦测试重新通过。
其全仓回归曾因磁盘失败；按用户继续授权把该环境缺口从实现准入中移出，
本次恢复构建空间后再次运行全仓基线。没有修改两份只读plan.md或Go源码/模块。

依据用户后续授权使用**只读Go源码/测试契约推导 + Rust行为验证**；没有运行Go，
没有宣称Go/Rust运行差分。源码/fixture SHA256、来源HEAD与计划基准记录在
[provenance.json](logs/task_059/provenance.json)，Go输出及其哈希明确为null。
[fixture](fixtures/task_059.json)记录18个案例、默认输入、4种placement布局和额外边界；
预期来自 `selectNewPeer`、`allowLeader`、`selectAvailableLeaderStore` 和
`TestScatterPreservesPlacement`、`TestScatterReservesPeerRole`、`TestScatterMixedLearner`
等源码契约，不由Rust实现生成。Go现有TestMain只是环境入口，
TestSomeStoresFilteredScatterGroupInConcurrency的源码明确不验证scatter结果；
本任务不把这些名称当运行覆盖。TestStoreLimit的操作限流属于后续操作接线范围。

本任务迁移普通/admin单region目标规划，复用真实StoreStateFilter、ExcludedFilter、
DistinctScoreFilter/RuleFitFilter和canonical fit_region。输入是宿主已解析的apply-range规则；
不自行复制RuleManager状态。选择不写组计数；真实操作生成/准入、批处理及内部请求
CPU策略不是该入口的职责。操作兼容性测试实际调用既有RangeOperatorFactory，
断言AddLearner/TransferLeader/RemovePeer目标和真实allocator分配的ID。
无MetaStore I/O，因此没有宣称存储故障注入覆盖。

[独立测试](../../../pkg/schedule/scatter/task_059_test.rs)显式注册5个有效测试：

1. `task_059_behavior_cases`：18个fixture案例，15正常/回退、3错误；引擎隔离、组隔离、
   计数相等保留、过载源搬移、避让/无候选、removed/busy过滤、learner/witness、
   follower规则/demoting/incoming角色、leader计数和源快照不变。
2. `task_059_planned_placement_is_refitted`：rules开/关 × E/F有/无4案例。
   无E/F时只允许一个peer移到D，有E/F时移到D/E/F；最终3种位置且fit满足。
   连续更新独立leader/peer布局，计数快照不变。
3. `task_059_reserved_peer_keeps_its_role`：预留另一个peer后重新选择，
   learner与witness保留原ID/角色，新增voter ID=0，目标leader为4。
4. `task_059_target_consumed_by_real_operator`：真实工厂构建操作，peer ID=900，
   新增目标4、转leader到4、移除源1。
5. `task_059_invalid_layout_and_fixed_order`：store0/重复store2两个错误；
   一次基准+20次重复完全相同；显式反转peer顺序时leader平局结果为3。

共26个不同语义案例（21成功/回退、5类型化错误），另22次确定顺序检查，
合计48次真实选择执行。5测试全部通过、0忽略；doc-tests=0不计行为数。

失败—通过证据：

- [初始缺API](logs/task_059/red-api.log)：退出101；缺少Selector/Counts。
  首版测试还误用了带数据PeerRole枚举的数值cast，已修正；这个编译错误不算行为证明。
- [placement变异](logs/task_059/red-placement.log)：使用旧region构造后续guard，
  编译成功；4测试通过、1失败，得到[4,5,6]，期望[2,3,4]。
- [选择变异](logs/task_059/red-ranking.log)：强制保留原peer，编译成功；
  1测试通过、4失败，包括主表、placement、预留角色、真实operator消费。
- 恢复后[聚焦](logs/task_059/focused.log)及[包级](logs/task_059/package.log)：
  各退出0，5通过、0失败、0忽略。

最初只有约125MiB空间，全仓重试退出101（ENOSPC）。后续用户授权优化可重建缓存，
确认无活跃Cargo持有者后，动态原子mkdir领取槽位1，
只清理该槽中mtime超过24小时的过期incremental缓存，保留已完成rlib/依赖产物、
其它槽位和用户数据。恢复到8,592,703,488字节可用；
[清理日志](logs/task_059/recover-space.log)记录精确目录列表和结果。
没有采用全局关闭debug/incremental以免无谓重编全部依赖；优先复用既有编译结果，
全仓测试设置CARGO_BUILD_JOBS=1控制峰值。

所有阶段由[run.py](logs/task_059/run.py)动态领取共享槽位；实际均为槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划/任务/会话/PID；正常/失败时子进程退出后释放，异常时终止并等待
子进程组后释放，不删除其他锁；缓存保留。每次Rust修改后先cargo fmt --all。
确切命令/退出码均有同名JSON及stdout/stderr日志；只清理日志行末空白。

最终基线结果、只读审计和交付状态见后续记录。


最终基线（同目录JSON记录确切命令、退出码、slot及target）：

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-scatter --locked --lib task_059 -- --nocapture` | 0 | 5测试、18表驱动案例，加4个placement等边界 |
| `cargo test -p astersql-pd-schedule-scatter --locked` | 0 | 5测试通过，doc-tests=0 |
| `cargo metadata --format-version 1 --no-deps` | 0 | canonical crate解析成功 |
| `cargo fmt --all -- --check` | 0 | workspace格式通过 |
| `cargo check --workspace --locked` | 0 | workspace编译检查通过 |
| `CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 0 | 全仓通过，数量见counts.json |

[全仓日志](logs/task_059/workspace-test.log)与[统计](logs/task_059/counts.json)保存实际结果：
1527项非doc测试通过，加2项compile-fail doctest，共1529通过、0失败；
1项既有`tso_processes`默认忽略（要求独立Go/Rust进程二进制与专用脚本），
不计有效通过，也不作为本任务证明。任务059没有忽略测试。
现有server/core_region_adapter.rs:134的deprecated cpu_usage警告未改动。
本次完整全仓命令也补齐前置58此前缺失的环境回归证据，保持任务58文件及证据只读。

[审计脚本](logs/task_059/audit.py)/[结果](logs/task_059/audit.json)：
全部1101个Go/模块文件清单和SHA256不变、Go Git状态为空、两份plan.md不变、
链接有效、git diff --check退出0。Cargo.lock远程条目逐字保持，只增加本地scatter crate。
格式化没有产生任务范围外Rust差异。保留开始前已有的其它编号任务删除；
提交只包含本任务生产/测试/Cargo/证据文件，编号任务状态/删除不纳入提交。
