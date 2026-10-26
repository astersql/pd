# 任务087：热点排名第一版

状态：已完成。

前置86核验：[task-086](task-086.md)、提交 `01e3b8180`、canonical
HotSolution与StoreLoadPred API，以及6聚焦/86包级/1635 workspace有效测试证据。
读取两级只读plan、oracle-index、ADR-001、移植指南与仓库规则。
按用户后续明确授权，用**只读Go源码/测试推导 + Rust真实行为及故障验证**
替代缺失的Go运行oracle/运行差分准入。**没有运行Go测试，没有Go运行输出或运行差分。**

## 实现与来源

生产路径：[hot_rank_v1.rs](../../../pkg/schedule/schedulers/hot_rank_v1.rs)，
[领域说明](../../../pkg/schedule/schedulers/hot_rank_v1.rs.md)，
[测试](../../../pkg/schedule/schedulers/task_087_test.rs)。
HotRankV1::{new,progressive_rank,better_than,check_loads,check_history,is_tolerance,
hot_decline_ratio,filter_uniform_store,rank_to_dim_string,is_available,
need_search_revert_regions,set_search_revert_regions}通过lib.rs公开。
没有新增Cargo依赖、feature、Go或模块文件变更。

[provenance](logs/task_087/provenance.json)记录来源提交、只读Go实现/测试SHA-256和
[fixture](fixtures/task_087.json)哈希。fixture在实现前冻结，evidence_kind为
source_assertion，go_runtime_executed=false，所有输入输出无归一化。
rank矩阵、容差和择优期望来自hot_region_rank_v1.go及hot_region_solver.go的
实际公式与分支；hot_region.go固定pendingAmpFactor=2、stddev=0.1、split rank=5。
getPlacementLoadState对v1关闭placement-v2，因此uniform使用全局stddev。

只读核实已有测试：TestHotWriteRegionScheduleWithPlacementScope明确覆盖v1保持全局
expectation；TestHotWriteRegionScheduleWithPendingInfluence显式选择v1并将pendingAmpFactor
设为0来测试调度pending；TestHotWriteRegionScheduleWithLeader验证leader循环与选举，
但不代替所有v1分支覆盖。TestSkipUniformStore在rank_v2测试中，不能冒称v1运行覆盖。
没有将上述测试名字作为执行证据。本任务直接从实际v1分支冻结预期。

## 真实行为与故障

6个独立测试，全部使用真实StoreLoadDetail/StoreLoadPred、HotPeerStat和HotSolution，
没有mock调用计数或替代领域对象。此层无新增I/O、持久化、线程、时间或随机边界。

- task_087_behavior_cases：15组冻结fixture，包含完整3×3改善矩阵、最小热速率前/边界、
  write-leader相等与交叉、pending阻断、负向revert早退；比较rank与可用性。
- task_087_priority_history_and_faults：write-leader×strict四组合，source/destination、
  两维history、空history、严格相等拒绝；短expectation在真实历史路径panic被捕获，
  无效/重复维度返回类型化错误，CPU/query实际负载排名。
- task_087_uniform_and_labels：双维uniform、第一维0.05/第二维0.1严格边界、全部rank、
  expectation关闭和维度标签。
- task_087_candidate_ordering：无旧候选、split、rank优先、revert有无、source/destination
  优先于peer、rank1–4收敛与振荡择优、rank0/-1拒绝、同一候选及相等候选保留旧值。
- task_087_ratio_and_tolerance_boundaries：great ratio边界、非hot维度、负净rate与分母
  clamp到1、混合正负rank0、pending严格边界/前后、反向源目的及revert搜索更新仍禁用。
- task_087_discrete_store_and_peer_ties：100-byte/10-key bucket相等、write-leader原始速率、
  源min相等后pending、leader count优先、普通资源忽略count、负数向零截断及目标max相等择优。

构造器校验维度；历史样本缺失和未初始化peer cache沿用Go索引/调用顺序前置条件。
同一个main peer以对象身份比较，revert presence沿用HotSolution的完整候选结构约束。
本任务仅实现v1排名领域，不宣称完成后续完整hot Schedule或operator创建。

## 红灯及验证

red-missing-api：同一测试首先因缺失API编译失败，退出101，不算行为红灯。
[red_behavior.py](logs/task_087/red_behavior.py)将真实生产代码“双维改善rank4”故意改为-1，
运行同一task_087过滤：2通过/2失败，退出101；better-better实际-1、期望4，
CPU/query真实负载rank也失败。finally逐字节恢复生产文件；随后新增边界测试并格式化。
正常实现下6个测试全部通过，未忽略任何本任务测试。

所有Cargo通过[run.py](logs/task_087/run.py)动态原子mkdir领取共享槽位1–10，
记录plan/task/session/PID并继承绝对CARGO_TARGET_DIR；异常/中断等待进程组退出后清理自身锁。
本任务实际全部使用**槽位1**，绝对路径为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，保留缓存。
命令/退出码/环境保存在同名JSON记录，完整stdout/stderr压缩于同名log.gz。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 本任务Rust文件已格式化 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_087 -- --nocapture` | 0 | 6通过/0失败/0忽略；15组fixture及上述边界/故障 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 92通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过；既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1641通过/0失败/1既有忽略，含2个文档测试 |

[有效计数](logs/task_087/counts.json)按真实test-result逐组汇总；不把零测试组当作行为证据。
既有忽略为tso_processes；[聚焦日志](logs/task_087/green.log.gz)、
[包级日志](logs/task_087/package.log.gz)、[workspace日志](logs/task_087/workspace-test.log.gz)、
[行为红灯](logs/task_087/red-behavior.log.gz)保留原始合并stdout/stderr。
所有测试进程及其子进程已结束，自己的槽位锁已释放，缓存保留。

`python3 docs/rust/schedule-execution/logs/task_087/audit.py` 与 `git diff --check` 均退出0。
[audit](logs/task_087/audit.json)对比开始快照的1119个Go/模块/plan文件SHA-256并扫描untracked，
无任何差异，Go状态为空。来源Go与原计划基准完全一致，provenance逐文件记录。
全workspace复用已有只读Go etcd fixture（binary与WAL位于槽位），不注入failpoint，
不运行Go测试；proto复用既有锁定生成缓存。本任务没有新增或改写任何Go源码/模块文件。
相对文档链接、生产公式、测试与注册已逐项审查；整体/包级plan和原始主题未修改。
任务78/80/81/82/83/84/85/86的既有删除不纳入本任务提交。
