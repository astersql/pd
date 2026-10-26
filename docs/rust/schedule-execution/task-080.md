# 任务080：慢网络检测候选判定

依赖批次79由提交 `0073df8fd`、[task-079](task-079.md) 与 canonical Rust
core/scheduler API 核实。整体与包级 plan.md 只读。
遵循用户对整个计划的补充授权：使用 **Go只读源码/测试推导 + Rust真实行为及故障验证**
替代缺失的Go运行oracle。已核查oracle-index.json，未将其中其它任务运行结果当成
本任务的oracle；本任务没有Go运行输出、二进制哈希或运行差分。

生产路径：[evict_network_detect.rs](../../../pkg/schedule/schedulers/evict_network_detect.rs)，
[领域说明](../../../pkg/schedule/schedulers/evict_network_detect.rs.md)。
API：`detect_network_slow_candidates`、`build_problematic_network_map`、
`should_skip_store_evaluation`、`is_network_slow_store`、`filter_network_slow_scores`。
公开 `ProblematicNetwork` 为对称图；恢复输入只用真实恢复map的键集合。
复用真实BasicCluster、StoreInfo、StoreStats；无新增Cargo依赖、feature或锁文件变化。
纯候选判定没有存储I/O或操作器生成，不伪造此处并不存在的持久化故障。
网络暂停、驱逐及恢复状态机由后续任务处理。

## 来源与期望

源码基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[provenance](logs/task_080/provenance.json)记录当前/基准Go源码和相邻测试哈希，
两者均与基准相同，并保存[fixture](fixtures/task_080.json)哈希。
fixture期望手工从只读Go源码/测试推导，没有从Rust输出反推。

实际相关Go测试为 `TestNetworkSlowStore`、
`TestNetworkSlowStoreSkipsRemovedStore`、`TestNetworkSlowStoreIgnoresKnownRemovedTargets`、
`TestNetworkSlowDenominatorIgnoresRemovedRecoveryEntries`、
`TestNetworkSlowStoreUsesOnlyLiveStores`、`TestNetworkSlowStoreReachLimit`；
依据helpers逐分支补充95/10阈值、未知目标、图边完整性和缺失评分的期望。
这些测试位于failpoint suite内，未裸跑。
任务原列的TestEvictSlowStoreBatch/BatchLimit/RecoveryTime是磁盘或批处理范围，
不能代表网络候选覆盖；TestCalculateAvgScore是恢复均值helper，本任务不实现或宣称验证它。

图构建至少需要2个过滤后分数，问题阈值>=95；候选至少3个分数，
满足live共识或问题图以外至少2个>=10的波动分数。零分仍计入完整评分。
已removed节点不充当候选/报告者/分母，Removing仍计入；已知removed目标过滤，
未知目标保留。恢复键过滤，只有live恢复节点从分母扣除。
候选返回排序仅消除Go map顺序，无评分裁剪或错误忽略；无时间/随机归一化。
检查前后均不改恢复集合、磁盘状态、network triggers或evicted引用。
最终再次查询注册/removed状态，保留Go一样的剩余并发窗口，不宣称锁住执行。

## 测试驱动与故障证据

- `task_080_behavior_cases`：22/22固定案例，含empty/1/2评分、94/95问题阈值、
  9/10波动阈值、歧义边、全peer共识、u64::MAX、未知目标、恢复候选、
  过滤后不完整、removed候选/目标/报告者、Removing、live分母、零分完整评分、
  多候选稳定排序、恢复报告者排除、单评分不建边。
- `task_080_filter_graph_and_skip`：过滤精确输出、原始StoreStats不被修改、恢复报告者
  不建图、对称边逐项比对、正常/恢复/无图节点跳过。
- `task_080_consensus_and_ambiguous_edges`：移除/未知恢复键不重复扣分母，live恢复键
  正确扣除，同时仍需3个过滤评分；非共识且波动不足不误报。
- `task_080_heartbeat_loss_and_removed_snapshot`：真实core对象替换评分、清空心跳评分、
  节点移除和删除，验证新检测为空且原Arc快照保留旧评分。故障边界不使用mock。

[red-api](logs/task_080/red-api.log.gz)：先注册测试，缺API编译失败，退出101，
不作为行为红灯。[red-behavior](logs/task_080/red-behavior.log.gz)：最小实现后，
临时把 `score >= 95` 改成 `score >= 96`，同一fixture的issue-95断言得到[]而非[1]；
图边和心跳故障测试也失败，1通过/3失败，退出101。已恢复正确阈值。
[focused-final](logs/task_080/focused-final.log.gz)：4通过/0失败，22/22案例。

## 验证结果与审计

状态：已完成。
每阶段由[run.py](logs/task_080/run.py)在仓库根按原子mkdir从1–10动态领取槽位，
记录计划、任务、session与持有PID；所有阶段实际均领取 **槽位1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
脚本等待Cargo和使用产物的子进程结束后，正常/失败/中断均释放自己的锁，缓存保留。
所有Rust改动后先cargo fmt --all，已审查差异，没有改动其它Rust生产模块。
确切命令、退出码、环境、槽位见同名JSON；合并stdout/stderr原样gzip保留。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_080 -- --nocapture` | 0 | 4通过/0失败，22/22固定案例及图/过滤/故障断言 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 56通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1605通过/0失败，1既有忽略 |
| `git diff --check` | 0 | 通过 |

[counts](logs/task_080/counts.json)来自实际测试输出汇总，workspace计数含2个文档测试；
既有忽略为tso_processes。[audit](logs/task_080/audit.json)确认1099个Go/模块文件
及两份只读计划的路径集合、内容哈希和Git状态与开始一致（含untracked清单），
槽位1锁已释放。源码/fixture哈希与运行输出分开保管，无Go运行证据被伪造。
提交只包含本任务实现、测试、接线、领域说明和证据，保留任务78原有删除。
任务80编号文件记录完成后删除，不修改上级plan或原始主题任务。
