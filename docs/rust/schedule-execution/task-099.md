# 任务099：范围散射调度

状态：已完成。

批次98依赖已核实：提交 `c26dd4042`、canonical RandomMergeScheduler 实现、
[task-098](task-098.md) 的145项包级及1694项workspace通过证据存在。本批次串行执行，
仅根AGENTS适用；已读整体/包级计划、ADR、依赖基线、移植指南和Copilot规则。
开始时78、80–98任务文件已经删除，与本任务无关。两个plan.md与原始主题保持只读。

用户明确授权以**只读Go源码/测试推导 + Rust真实行为和故障验证**替代缺失的
Go运行oracle/运行差分。本任务没有Go运行oracle，不声称Go/Rust运行差分；没有运行
Go工具、测试或failpoint命令。[provenance](logs/task_099/provenance.json)记录实际
source_commit、基准commit、Go源码/测试、oracle-index和fixture SHA-256，
runtime_oracle=null。[fixture](fixtures/task_099.json)为源码推导场景。

Go TestScatterRangeBalance覆盖placement开关、50个区域、5节点、100轮应用操作器后
leader<=12与peer<=32；本任务采用其真实组件与局部均衡意图，不声称复现了该Go运行或
100轮收敛断言。额外配置/故障、严格限额、leader优先及回退预期来自scatter_range.go，
抽样与投影来自range_cluster.go，pending/零大小行为来自balance_region.go及评分公式。

## 生产路径与边界

[scatter_range.rs](../../../pkg/schedule/schedulers/scatter_range.rs) 提供
`ScatterRangeConfig::from_args`、`ScatterRangeScheduler::{new,name,config,encode_config,
allow_balance_leader,allow_balance_region,is_schedule_allowed,schedule,persist,reload,update}`。
[领域说明](../../../pkg/schedule/schedulers/scatter_range.rs.md)解释范围、持久化和错误语义。
lib显式注册生产模块及独立测试，无Cargo依赖或锁文件变化。

每次重建真实RangeCluster，局部复制配置并固定tolerance=2，宿主原值10000保持不变。
复用真实leader/region引擎及retry状态；custom名称跳过普通balance调度器的排除范围，
小区域允许调度。共享OpRange计数分别检查leader/region限额，leader成功立即返回，
否则回退region；只有首个operator改description并附加RANGE，保留其余批次项。
`Operator::decorate(&mut self,...)`仅在发布前独占修改元数据，不重建operator或丢失步骤、
priority、health、影响量和additional_info。typed BuilderError保留给宿主诊断；plans丢弃。

name/start/end builder只校验恰好3项，不额外拒绝空值或反向范围，不URL解码原始字符串。
update拒绝改名，先发布新键再persist；写失败本地新键保留、持久化旧值不变。reload
失败保留旧配置，成功可更新配置名但不改变实例名/存储路径。HTTP、metrics发布、
工厂注册与operator提交由后续宿主/适配层承担。

真实组件包括BasicCluster/RangeCluster、StoreStateFilter、RuleManager、RegionLabeler、
StatisticsSnapshot、OpInfluence和PeerOperatorFactory；MetaStore使用MemoryStore正常
后端，仅在read/transact I/O边界注入Unavailable。固定时间1000000000000ns，random
index=0/shuffle恒等；同分数目标只允许合法候选集合，不忽略操作、kind、错误或状态差异。

## 行为及红灯证据

`task_099_test.rs`显式注册2项测试，无忽略：

- `task_099_behavior_cases`：12条冻结源码推导场景 × placement开关 × classic/v2 =
  **48条有效案例**。覆盖leader优先、leader-only、region-only、leader无操作回退、
  两类限额、allocator故障、范围miss、pending回退、开放end、down拒绝、零大小无操作、
  空范围。宿主全局store评分故意与范围内分布不同，验证使用局部投影。断言操作所属
  区域、description、kind、leader源/目标、peer目标/分配ID、region influence守恒、
  初始confver影响=0、额外leader批次项未附加RANGE、plans为空、宿主配置/影响不变。
- `task_099_config_and_faults`：参数项数、空名/反向键接受、原始+/%保留、重命名拒绝、
  persist/update/reload I/O失败、写失败本地/持久化分离、成功重载恢复、坏JSON/字段类型
  拒绝、空键更新、reload名称变化且实例名稳定、6组严格限额边界（含u64::MAX）。

早期initial/focused/focused2日志保留测试设计修正：ProviderSnapshot默认max_replicas=0，
需显式设置3；pending分支在Go中允许region回退而非全部拒绝；全零大小region评分相同
不产生操作。没有把这些期望修正当成生产红灯修复。

[red_behavior.py](logs/task_099/red_behavior.py)刻意把本地tolerance=2改为10000，格式化后
运行同一behavior测试；leader-first断言无操作 != 预期leader，0通过/1失败，Cargo
exit101。脚本确认预期失败exit0并在finally逐字节恢复生产源码。随后focused3为
2通过/0失败，明确证明测试能识别错误行为，不用缺API/编译错误冒充行为红灯。

## 最终验证

全部Cargo入口经[run.py](logs/task_099/run.py)原子mkdir动态领取共享槽位1–10，记录
计划/任务/会话/PID并设置绝对CARGO_TARGET_DIR；异常时结束并等待子进程组，释放
自有锁、保留缓存。各阶段实际槽位均为 **1**：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

精确命令/环境/退出码见各同名JSON；完整stdout/stderr见对应log.gz。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅任务相关格式化差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_099 -- --nocapture | 0 | 2项测试，48条行为案例 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 147通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | workspace元数据通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过，仅既有cpu_usage弃用warning |
| CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked | 0 | 1696通过/0失败/1既有忽略，包含2项文档测试 |

[counts.json](logs/task_099/counts.json)从完整日志汇总有效测试数量。
[audit.json](logs/task_099/audit.json)对1119个Go/模块/plan文件的开始/结束SHA-256逐项
核对，包含untracked扫描；无变化、Go Git状态为空、git diff --check退出0。所有验证及
使用槽位产物的进程已退出，自有slot-1.lock已释放，缓存保留。文档相对链接解析通过。
无必需验证缺口（按用户授权的源码推导验收口径）。任务文件状态/删除不纳入提交，
提交仅包含本任务生产实现、独立测试、必要operator元数据API、模块接线与证据。
