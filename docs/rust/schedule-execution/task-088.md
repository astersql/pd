# 任务088：热点排名第二版

状态：已完成。

前置87核验：提交 `61372ce92`、[task-087](task-087.md) 中6个聚焦测试、92个包级
及1641个workspace测试的通过记录，canonical HotRankV1/HotSolution API 均存在。
读取整体/包级只读计划、oracle-index、ADR-001、移植指南及仓库说明。
用户后续明确授权使用**只读 Go 源码/测试推导 + Rust 真实行为及故障验证**代替
缺失的 Go 运行 oracle/运行差分。**本任务没有运行 Go 测试，没有排名 Go 运行输出或排名运行差分。**

## 实现与来源

生产路径：[hot_rank_v2.rs](../../../pkg/schedule/schedulers/hot_rank_v2.rs)，
[领域说明](../../../pkg/schedule/schedulers/hot_rank_v2.rs.md)，
[测试](../../../pkg/schedule/schedulers/task_088_test.rs)。
导出 HotRankV2::{new,score,progressive_rank,check_loads,check_history,is_tolerance,
filter_uniform_store,better_than,need_search_revert_regions,set_search_revert_regions,
rank_to_dim_string}、RankRatios、HotRankV2Result::{is_available}、v2_priority_cmp
和 HotRankV2Error。store 比较复用 v1，只扩大到 crate 内可见性。
没有新增 Cargo 依赖、feature、Go 或模块文件差异。

[provenance](logs/task_088/provenance.json)记录来源提交、三个只读 Go 文件和
[冻结 fixture](fixtures/task_088.json) SHA-256。22组评分 inputs/outputs 和9格排名矩阵
在实现前冻结，evidence_kind=source_assertion，go_runtime_executed=false，无归一化。
评分来源为 hot_region_rank_v2.go 的真实分支和公式，容差/历史/离散比较来源为
hot_region_solver.go。基准 balanced=.95、minRate=1、无pending、无topn限制。
源码中的3×3排名表符号注释与实际分支不一致，测试遵循实际 score>0 为改善的代码。

已只读核实任务列出的五个测试：
- TestHotWriteRegionScheduleWithRevertRegionsDimSecond：v1无候选，v2下一轮开启revert，
  rank1 revert 当前仍被禁用；新增更好区域后仅一个operator，关闭搜索。
- TestHotWriteRegionScheduleWithRevertRegionsDimFirst：rank3主/revert两operator及pending抵消。
- TestHotWriteRegionScheduleWithRevertRegionsAndPlacementRulesV2：有效/无效目标、scoped
  history拒绝和location labels拒绝四场景。
- TestHotWriteRegionScheduleWithRevertRegionsInSamePlacementScope：局部人口下两个operator。
- TestHotWriteRegionScheduleWithRevertRegionsDimFirstOnly：rank2候选下一轮搜索revert。

以上是源码阅读，未将测试名字当运行证据。完整 placement 匹配、operator 构造及
Schedule 接线属于后续 solver，不在此排名任务重复实现。此层接收真实 population
stddev 快照和 nth peer 速率；测试通过 canonical StoreLoadSummary 计算 scope stddev。

## 真实行为与故障

5个独立测试使用真实 StoreLoadDetail/StoreLoadPred、HotPeerStat、HotSolution 和
StoreLoadSummary，无mock调用计数、I/O、时钟、随机或持久化边界。
- task_088_behavior_cases：22组冻结评分（unbalanced的全部七分区、pre-balanced、balanced）
  和9格排名矩阵；总31组输入输出断言。
- task_088_ratios_pending_reverse_and_topn：balancedRatio clamp，topn减小门槛，
  负向净负载/reverse目标topn，pending总和0.5/1的严格边界及阻断，balanced区间两端，
  write leader只看第一维且score保持初始化零值。
- task_088_history_faults_and_uniform：源/目标任一维通过，write leader首维拒绝，
  相等严格拒绝，空history，短expectation panic被捕获，无效/重复维度类型化错误；
  uniform各rank、严格0.05/0.1、revert scope有无、expectation关闭及维度标签。
- task_088_revert_state_and_candidate_ties：5种best×7种current搜索状态，rank×revert可用性，
  四维×六score的bucket/score择优，rank1–4、相同候选、空旧值、revert个数、split、
  rank优先和源/目标store优先。
- task_088_priority_order_and_real_population：第一/第二优先级冲突、同bucket后次维择优、
  score优先于rate，write leader忽略次维，CPU/query真实负载，真实人口统计uniform，
  未初始化peer cache panic被捕获。

构造器拒绝无效维度；调用者保证缓存及history对齐，不将Go前置条件改写为默默通过。
完整main/revert候选生命周期和作用域选择由solver持有；此层无新增持久化故障边界。

## 红灯及验证

red-missing-api 首先记录同一行为测试缺失API编译错误，退出101，不计作行为红灯。
[red_behavior.py](logs/task_088/red_behavior.py)故意将双维改善的rank4改为-1，
同一过滤命令3通过/1失败，退出101，矩阵0/0明确实际-1、期望4。
finally逐字节恢复生产文件；之后补充优先级测试和类型化错误并格式化。
正常实现最终5通过/0失败/0忽略。

所有Cargo通过[run.py](logs/task_088/run.py)动态原子mkdir领取共享槽位1–10，
记录plan/task/session/PID，所有命令继承绝对CARGO_TARGET_DIR，失败/中断等待进程退出
再释放自己的锁。实际槽位和确切命令/退出码保存在各同名JSON文件。

本任务全部使用**槽位1**，绝对 `CARGO_TARGET_DIR` 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
所有测试及其子进程已结束，自己的槽位锁已释放，缓存保留。
原始合并 stdout/stderr 压缩保存在同名 `.log.gz`，命令环境与退出码在同名 `.json`。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅任务相关Rust格式化差异 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_088 -- --nocapture` | 0 | 5通过/0失败/0忽略，31组fixture及边界/故障 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 97通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1646通过/0失败/1既有忽略，包含2个文档测试 |

[有效计数](logs/task_088/counts.json)由[counts.py](logs/task_088/counts.py)从实际test-result
汇总，未把零测试组当作行为证据。既有忽略项为tso_processes。
[聚焦](logs/task_088/green.log.gz)、[包级](logs/task_088/package.log.gz)、
[workspace](logs/task_088/workspace-test.log.gz)和[红灯](logs/task_088/red-behavior.log.gz)
日志完整保留。

`python3 docs/rust/schedule-execution/logs/task_088/audit.py` 和 `git diff --check` 均退出0。
[audit](logs/task_088/audit.json)逐一比较开始快照的1119个Go/模块/plan文件SHA-256，
包括扫描untracked，无差异，Go状态为空；原始主题也无改动。
workspace测试复用已有只读Go etcd fixture源码编译二进制，输出/WAL位于领取槽位，
不运行Go测试或注入failpoint；proto复用现有锁定生成缓存。这些是workspace存储基线，
不是本任务的排名Go运行oracle。
文档相对链接、公式、测试/注册及差异已审查，整体/包级plan和原始主题均未修改。
已有任务78/80/81/82/83/84/85/86/87的删除不纳入本任务提交。
