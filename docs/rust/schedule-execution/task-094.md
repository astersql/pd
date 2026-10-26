# 任务094：热点读写调度接线

状态：已完成。

依赖93已核实：提交 `deb38f6ed`，真实 HotSplit 桶拆分 API 和
[task-093](task-093.md) 的5项聚焦、128项包级、1677项workspace成功证据。
已读两级只读计划、仓库/协作规则、ADR-001、依赖基线与移植指南；无更深AGENTS。
按用户后续明确授权使用**只读Go源码/测试推导 + Rust真实行为与故障验证**，
没有Go运行oracle，不声称Go/Rust运行差分。oracle-index未提供94的运行输出。

## 实现与边界

[hot_schedule.rs](../../../pkg/schedule/schedulers/hot_schedule.rs) 的
HotScheduler::{new,schedule,dispatch,min_interval,next_interval,is_schedule_allowed}，
HotResource、HotRankInputs、HotDispatchResult、HotScheduleError、HotScheduleDiagnostic。
[领域说明](../../../pkg/schedule/schedulers/hot_schedule.rs.md)。
复用4个真实 HotSolver 和 canonical statistics/core/config/operator，局部lib注册；
Cargo.toml/Cargo.lock无改动。HotSolver::prefers_result调用既有排序公式比较读赢家，
同步其[说明](../../../pkg/schedule/schedulers/hot_solve.rs.md)。

writePeer/writeLeader/readPeer/readLeader顺序与Go一致。任一读资源都求解leader和
peer，再按leader公式排序，平局先peer；写方向只求解对应路径。构造检查配置并
降级不支持的query/CPU优先级；host传入同轮已准入的候选/负载rank步长、随机word。
抽样取模资源列表；空列表typed error。四个solver分别保留反向搜索状态。
配置/统计快照更新时宿主应重建本轮scheduler（跨快照状态延续由后续宿主装配负责）。

禁用在dispatch内；调用方须先准备统计/cache快照，不能因禁用而跳过准备。
限额严格count < limit；下一间隔始终1.3秒，与上次间隔无关。无解及禁用提供
领域诊断，builder分配失败保留typed error，某一路失败不丢弃另一条成功解。
读方向返回排序后的真实operator offers，供95实现pending原子准入及失败备选。
当前不登记pending、不提交operator；93拆桶API不在此复制；完整集群/cache/
placement/桶拆分和pending的宿主全链装配仍在后续facade与集群任务验收。
本任务没有持久化外部写入，不引入MetaStore mock。

## 来源与冻结案例

[provenance](logs/task_094/provenance.json)记录源码commit及Go文件/fixture SHA-256；
[fixture](fixtures/task_094.json)显式source_assertion、runtime_oracle=null，
不归一化目标、步骤或错误。参考hot_region.go dispatch、randomType、
balanceHotReadRegions、balanceHotWritePeers/Leaders、GetNextInterval、
IsScheduleAllowed、resourceType，以及rank_v1/v2 betterThan。

只读审查指定Go测试：TestUpgrade/ReloadConfigKeepsPendingWeightDefault/
CreateSchedulerRejectsPendingWeightBelowOne分别验证85的历史配置、缺省pending权重
与校验；不能当作读写dispatch覆盖。TestGCPendingOpInfos验证pending GC，属于95。
TestSplitIfRegionTooHot验证读写桶拆分及pending抑制，93已有桶拆分领域证据，但
本任务不把它列作Go端到端运行通过。所需本任务route/forbid/interval/limit预期
从上述生产控制流推导，未运行依赖failpoint的Go测试。

5项真实测试显式注册，无忽略：

- task_094_behavior_cases：6条冻结fixture × v1/v2=12条。四资源各自成功，
  以及抽到readPeer但只有leader有解、抽到readLeader但只有peer有解；真实operator
  description、region、首步与目标store匹配。
- task_094_read_competition_and_fallback：v1/v2各3条，leader更佳、peer更佳、
  同负载平局，验证首选及真实可执行备选。
- task_094_intervals_limits_and_config：5条旧间隔边界、5条限额边界、最小间隔、
  typed非法配置。
- task_094_forbid_no_solution_and_faults：两公式下四资源禁用、allocator错误与
  无解诊断；空输入、空资源typed error、usize::MAX抽样、相反方向仍执行。
- task_094_sampling_and_partial_read_failure：v1/v2取模抽中readPeer，peer allocator
  故障时保留无需新ID的真实leader transfer；取模抽中writeLeader时不运行读路径。

红灯[red-api](logs/task_094/red-api.log.gz)缺HotScheduler API编译失败，退出101。
[red_behavior.py](logs/task_094/red_behavior.py)临时从读dispatch删除peer求解器，
同一behavior_cases得到NoSolution != None，competition实际1条 != 2条失败，
2通过/2失败，Cargo退出101；外层检测成功退出0，finally逐字节恢复源码。
最终5项测试使用恢复后的真实实现。源码推导不与运行oracle混淆。

## 验证记录

每阶段通过[run.py](logs/task_094/run.py)从1–10原子mkdir动态领取槽位；
记录任务/会话/PID；异常与中断先终止并等待子进程组再清自己的锁，保留缓存。
命令与exit code在同名JSON，完整stdout/stderr在log.gz。
实际各阶段均动态领取槽位 **1**，绝对 CARGO_TARGET_DIR：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
所有阶段已结束，自有锁已释放，缓存保留。

| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅任务相关Rust格式差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_094 -- --nocapture | 0 | 5通过/0失败/0忽略；12条冻结路由与6条读方案比较 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 133通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过；既有cpu_usage弃用warning |
| CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked | 0 | 1682通过/0失败/1既有忽略，含2项文档测试 |

[计数](logs/task_094/counts.json)由[counts.py](logs/task_094/counts.py)从完整日志解析。
[focused-final](logs/task_094/focused-final.log.gz)、
[package](logs/task_094/package.log.gz)、
[workspace-test](logs/task_094/workspace-test.log.gz)保留原始输出。

[audit.json](logs/task_094/audit.json)：开始/结束1119个Go/模块/plan文件SHA-256
逐一一致，含untracked扫描，Go Git状态为空，git diff --check退出0。
两个只读plan及原主题未修改。开始时78、80–93任务删除为既有变更，不纳入提交。
新增文件均在本任务范围；无Go/模块/Cargo锁变更。
