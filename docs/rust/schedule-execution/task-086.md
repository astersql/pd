# 任务086：热点方案负载投影

依赖85已核验：提交 `b39ce14fd`、[task-085](task-085.md)、真实 HotRegionParams /
HotRegionConfig 及其80个包级、1629个workspace测试证据。已读取两级只读plan、oracle-index、
ADR-001及移植指南。按用户后续明确授权，采用**只读Go源码/测试推导 + Rust真实行为及
故障验证**替代缺失的Go运行oracle准入。**未运行Go热点测试，无Go运行输出或运行差分。**

实现：[hot_solution.rs](../../../pkg/schedule/schedulers/hot_solution.rs)，
[领域说明](../../../pkg/schedule/schedulers/hot_solution.rs.md)，
[测试](../../../pkg/schedule/schedulers/task_086_test.rs)。API为
HotSolution::{new,calc_peers_rate,peers_rate,current_load,pending_load,extreme_load}、
effective_hot_pending_weight、HotLoadProjection::{summary_pending_influence,update_history_load_config}。
复用canonical statistics/store history、真实Operator、既有PendingInfluence及其时间权重计算，
没有复制生命周期实现。新依赖仅测试所需既有statistics-utils；Cargo.lock只加入路径包引用，
离线更新产生的pulldown-cmark漂移已恢复，无远程版本变化。

## 期望与覆盖范围

[provenance](logs/task_086/provenance.json)记录当前来源提交与6个Go文件的SHA-256；
它们和计划基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe` 相同。
[fixture](fixtures/task_086.json)标注source_assertion与go_runtime_executed=false，
冻结6组方案×4维度及15组生命周期原始输入/期望。所有期望在Rust实现之前从只读Go
公式/分支冻结，未由Rust输出生成。数值逐维比较，不忽略状态、错误或目标差异；
没有随机输入，时间以宿主单调时钟的相对秒数固定，source/destination顺序不归一化。

指定Go对照入口已逐条只读核实：TestUpgrade覆盖默认pending weight及旧配置优先级/rank，
TestReloadConfigKeepsPendingWeightDefault覆盖缺失weight的重载默认，
TestCreateSchedulerRejectsPendingWeightBelowOne覆盖0/0.5/-1拒绝与1接受；
前述配置契约主要由任务85验证。TestGCPendingOpInfos覆盖两种placement设置、move-peer/
transfer-leader、旧取消记录回收及未完成/刚取消记录保留，不覆盖所有负载或极值计算。
TestSplitIfRegionTooHot覆盖bucket split调度，其完整Schedule及split不在本任务范围。
方案、effective weight、summary与history期望分别来自hot_region_solution.go、
hot_region.go、statistics/store_load.go的具体实现。未将Go测试名称当成运行覆盖证明。

6个真实Rust测试：

- task_086_behavior_cases：6组冻结fixture×4维度，正/负/混合/零净rate、可选revert、
  round后相减、subset/empty维度，比较current/pending/extreme/cache，并验证重新计算清零。
- task_086_pending_lifecycle_and_faults：15组fixture覆盖created/started/success/canceled/
  replaced/timeout/expired，窗口前/恰好到期/之后/零窗口、summary保留或GC、真实影响量；
  另有2组不预先刷新status的自动过期/超时，summary直接刷新并在30秒后回收。
- task_086_summary_weights_missing_and_multiple_sources：6组配置（无、-1、0、0.5、1、2.5），
  比较8个origin维度及count；多源含缺失源仍给目标逐次累加、源目标均缺失不报错且保留pending、
  同源同目标抵消。使用真实READ StoreLoad::to_load_pred核验CPU映射及future/count。
- task_086_history_configuration：同配置保留采样、改配置清空、采样间隔前/边界、零duration关闭。
- task_086_cache_precondition：未初始化读取与非法维度panic被捕获；初始化后可用。
- task_086_cache_rolling_and_summary_cancellation：真实DimStat中位数优先于raw load、
  短向量回退、替换revert后重算、两条反向pending记录的8维及count抵消；取消立即不再
  影响、到期回收；空source记录沿用Go内循环不回收语义。

所有状态来自真实Operator，控制仅在Clock边界；本任务无持久化或外部I/O新增路径。
full Schedule、rank、split及metrics发布留在后续宿主；配置验证/保存/重载继续由任务85负责。

## 失败与通过验证

首次 `--locked` 发现新增路径依赖需更新lock，退出101；随后离线更新，仅保留路径包差异。
red-missing-api 为缺API编译失败101，不计行为红灯。
[red_behavior.py](logs/task_086/red_behavior.py)临时反转Rust extreme条件并清零pending weight，
运行同一聚焦测试：2通过/4失败，退出101，具体正rate案例实际极值(100,20)而期望(90,25)，
其余影响量断言也失败。finally逐字节恢复生产文件并重新格式化。
恢复后同一6个测试全部通过，无忽略。原始stdout/stderr合并保存到同名 `.log.gz`，
命令、退出码、环境及实际槽位保存在每个JSON记录；有效测试汇总见counts.json。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo generate-lockfile --offline` | 0 | 仅增加statistics-utils路径包引用 |
| `cargo fmt --all` | 0 | 只保留任务86相关Rust差异 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_086 -- --nocapture` | 0 | 6通过/0失败/0忽略；6×4方案维度、15生命周期fixture及上述边界/故障 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 86通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过；既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1635通过/0失败/1既有忽略，含2个文档测试 |

[计数](logs/task_086/counts.json)逐组汇总真实输出；既有忽略为tso_processes。
[聚焦](logs/task_086/green.log.gz)、[包级](logs/task_086/package.log.gz)、
[workspace](logs/task_086/workspace-test.log.gz)、[行为红灯](logs/task_086/red-behavior.log.gz)
保存完整日志，不以零测试组替代有效行为计数。

[run.py](logs/task_086/run.py)每阶段从1–10原子mkdir动态领取槽位，记录plan/task/session/PID，
继承绝对CARGO_TARGET_DIR；失败/中断等待子进程退出后清理自身锁、保留缓存。
本任务所有阶段实际领取**槽位1**，`CARGO_TARGET_DIR` 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，所有子进程结束且自身锁已释放。
workspace已有etcd fixture入口沿用前序只读审查结论：复用既有Go源码，binary与WAL落槽位，
不注入failpoint、不运行Go测试或改写Go/模块文件；proto复用既有锁定生成缓存。

`python3 docs/rust/schedule-execution/logs/task_086/audit.py` 与 `git diff --check` 退出0。
[audit](logs/task_086/audit.json)与[开始快照](logs/task_086/before.json.gz)核对1119个
Go/模块/plan文件SHA-256，包括untracked扫描，全部一致；开始Git状态另存
initial-status.txt.gz。生产、测试、fixtures、Cargo接线、相对文档链接已审查。
无新增Go差异、未修改两级plan或原主题；其它任务78/80/81/82/83/84/85已有删除不纳入提交。
全部必需验证通过，状态：已完成。
