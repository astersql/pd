# 任务098：随机合并调度

状态：已完成。

批次97真实依赖已满足：提交 `0f5b5b29b`、canonical label 实现与
[task-097](task-097.md)的143项包级/1692项workspace通过证据存在。已读取整体与包级
计划、ADR/依赖基线/移植指南及Copilot规则；仅根AGENTS适用。开始时已有78、80–97
任务文件删除，与本任务无关，不纳入提交。两个plan.md及原始主题保持只读。

用户后续明确授权允许以**只读Go源码/测试推导 + Rust真实行为和故障验证**替代缺失的
Go运行oracle/运行差分。本任务没有Go运行oracle，也不声称Go/Rust运行差分。
[provenance](logs/task_098/provenance.json)记录源码commit、源码/已有测试/oracle-index与
[冻结fixture](fixtures/task_098.json)的SHA-256，runtime_oracle=null。未运行Go工具、
测试或failpoint注入。TestRandomMergeSchedule本身仅覆盖rules开关、非完整副本拒绝、
两项MERGE操作器及提交后限额拒绝；范围/方向/健康/热点等额外案例来自只读生产源码。

## 生产路径

[random_merge.rs](../../../pkg/schedule/schedulers/random_merge.rs)提供
`RandomMergeConfig::{new,from_args,ranges}`、`RandomMergeContext` 和
`RandomMergeScheduler::{new,config,encode_config,is_schedule_allowed,schedule}`。
[领域说明](../../../pkg/schedule/schedulers/random_merge.rs.md)记录边界与输入契约。
lib显式注册生产模块和独立测试；Cargo新增已有checker依赖并将labeler升级为生产依赖。
Cargo.lock仅增加本地checker边；离线生成时发现pulldown-cmark远程版本漂移，已还原。

选一个通过LOW/MoveRegion源过滤的store，范围内抽样一个无pending/down的leader
region，再选前/后相邻区域。单向只选后邻居；双向按显式随机parity选前或后，后为空
时回退前；选中目标不允许时不重试另一个邻居。目标可在源配置范围外。
健康、完整副本、热点与共享checker::allow_merge同时通过才创建操作器。
不引入自动merge checker的大小、split冷却或checkPeerStore条件。
真实RangeOperatorFactory生成active/passive配对、按需对齐peer，两者LOW优先级；
typed BuilderError保留，失败无部分配对。限额count < limit由宿主单独调用。
HTTP、registry、metrics发布和operator提交留给宿主/适配层。

复用真实BasicCluster、StoreStateFilter、replication fit、StatisticsSnapshot、RuleManager、
RegionLabeler与RangeOperatorFactory；placement/label存储采用真实MemoryStore endpoint。
配置没有新增持久化I/O。时钟固定1000000000000纳秒；随机index、shuffle、方向parity
作为边界输入。Go store map顺序不确定，仅排序store ID稳定该顺序，不忽略kind、目标、
命令、影响或错误。JSON键范围保留null/空数组、lowercase hex及既有query-unescape。

## 测试与失败证据

`task_098_test.rs`显式注册2项测试，无忽略：

- `task_098_behavior_cases`：28条冻结源码推导场景 × classic/v2命令 = **56条有效案例**。
  覆盖前/后、单向、尾部回退、范围miss/目标范围外、断联源仍允许、busy源拒绝、随机
  选到无leader的源、未完整副本、placement正常/边界拒绝、label deny/边界拒绝、
  source/target热区、两侧pending/down/learner/joint、peer对齐与ID分配失败。
  断言目标/源ID、description、MERGE kind、LOW、active/passive、完整merge目标metadata、
  classic/v2命令、confver=0、region/leader count影响与passive完成状态；相同peer集时
  精确单步MERGE。联合状态返回OriginJoint；I/O式allocator故障返回Allocation且调用一次。
- `task_098_config_and_limits`：CLI默认、hex JSON、null、坏query编码、损坏JSON与5项
  严格限额边界（含0与u64::MAX）。

初次缺API编译失败exit101保存在red-initial，不视作行为红灯。早期focused日志保留测试
接线修复（Operator无Debug、OpKind原始位图）、hex lowercase、MoveRegion允许断联源、
不同peer集影响字段及Rule的显式Voter配置修正；未修改生产行为来迁就错误期望。
[red_behavior.py](logs/task_098/red_behavior.py)反转one-way判断，同一行为测试在前邻居
场景得到target 3 != 1，0通过/1失败，Cargo exit101；外层确认预期失败exit0。
finally逐字节恢复生产文件，随后最终聚焦与包级通过。

## 最终验证

所有Cargo入口均经[run.py](logs/task_098/run.py)原子mkdir动态领取共享槽位1–10，
写入计划/任务/会话/PID，设置绝对CARGO_TARGET_DIR；异常时终止并等待子进程组，再
释放自有锁。各阶段实际均为槽位 **1**：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

命令、环境与退出码保存于同名JSON，完整stdout/stderr保存在log.gz。

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 本任务Rust格式化 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_098 -- --nocapture | 0 | 2项测试，56条行为案例 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 145通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过，既有cpu_usage弃用warning |
| CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked | 0 | 1694通过/0失败/1既有忽略，含2项文档测试 |

[audit.json](logs/task_098/audit.json)逐一核对1119个Go/模块/plan文件的开始/结束SHA-256，
包含untracked扫描；无变化、Go Git状态为空、git diff --check退出0。

[counts.json](logs/task_098/counts.json)由完整日志解析有效测试数。所有验证和槽位产物
进程已退出，自有锁已释放，缓存保留。文档本地链接均解析通过，差异只含本任务
实现、测试、证据和必要依赖接线。没有必需验证缺口（按用户授权的源码推导验收口径）。
