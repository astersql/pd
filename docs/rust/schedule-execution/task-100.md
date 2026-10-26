# 任务100：热点随机扰动调度

状态：已完成。

批次99真实依赖已核实：提交 `5a666a2dd`、`ScatterRangeScheduler`、
[task-099](task-099.md)的147项包级及1696项workspace通过证据存在。本批次串行执行，
没有子代理。两个上层plan.md及原始主题保持只读；开始时78、80–99任务文件已删除，
这些删除与本任务无关。

按用户后续明确授权，采用**只读Go源码/测试推导 + Rust真实行为和故障验证**替代缺失
Go运行oracle/运行差分；没有Go运行结果，也没有执行Go工具、测试或failpoint命令。
[provenance.json](logs/task_100/provenance.json)记录source_commit、基准、Go来源、
oracle-index及fixture的SHA-256，runtime_oracle=null。

## 生产实现与依据

[shuffle_hot.rs](../../../pkg/schedule/schedulers/shuffle_hot.rs)提供
`ShuffleHotConfig::from_args`、`ShuffleHotScheduler::{new,config,encode_config,
is_schedule_allowed,schedule,dispatch,persist,update,reload}`，lib显式注册生产模块和
独立测试，无Cargo依赖或锁文件变更。[领域说明](../../../pkg/schedule/schedulers/shuffle_hot.rs.md)
记录宿主统计、HTTP和工厂边界。

来源shuffle_hot_region.go决定：read/write leader资源才调度；随机一个hot peer；
missing/down/pending跳过；使用当前leader为迁出源；真实状态、驱逐、peer排除、placement
过滤；无目标即结束；真实move-leader builder生成REGION|LEADER、Low操作。限额为
hot/region/leader三类严格小于。持久化更新失败回滚，reload失败保留旧值。
init.go决定默认limit=1及参数个数不为1时保持默认。hot_region.go决定四类资源顺序。
Go `TestShuffleHotRegionScheduleBalance` 覆盖placement开关、拓扑目标拒绝及最终leader
转入新peer；本任务据此设计真实组件用例，不声称复现Go随机100轮运行。

宿主提供四类真实StoreLoadDetail快照、外部时间、分配与随机输入，复用真实BasicCluster、
RuleManager、StoreStateFilter、HotRegionEvictedTargetFilter、PeerOperatorFactory。
候选store ID排序后取random modulo，消除Go/Rust map顺序差异；详情顺序显式输入。
固定时间100、分配peer ID100；每次独立resource/peer/target word，同一peer word应用到
当前详情遍历。区域统计peer刻意位于store2，断言迁出实际leader store1。
缺失leader/source时typed Source替代Go潜在nil解引用，builder错误保留给宿主诊断。

配置更新为typed domain API，HTTP数字转换留给适配层；空对象reload limit=0。
MetaStore正常路径使用MemoryStore，只有read/transact I/O边界注入Unavailable。

## 真实测试及红灯

[fixture](fixtures/task_100.json)包含18条源码推导案例，分别运行placement开关和
classic/conf-change-v2，共 **72条有效行为案例**。覆盖read/write、peer资源不操作、
固定随机目标、无热点、missing/down/pending、无合法目标、slow-trend驱逐、allocator
失败、缺失源/leader、只抽一个peer不扫描其他peer、拓扑退化拒绝、offline目标拒绝。

`task_100_test.rs`显式注册3项测试：

- `task_100_behavior_cases`：断言数量、region、description、kind、Low优先级、peer ID、
  迁出源、新peer/leader目标、初始confver影响、peer/leader影响守恒和真实heartbeat命令。
  allocator故障断言typed BuilderError::Allocation。
- `task_100_config_and_limits`：默认、参数个数、0、溢出/负数/空参数拒绝，9组严格限额
  边界含u64::MAX。
- `task_100_persistence_faults`：保存/重建恢复、写失败本地旧值保留、读取失败、
  坏JSON/类型/负值拒绝保留、空对象重载零值。

initial日志记录缺API编译失败（exit101），不以此冒充行为红灯。focused2记录目标顺序
断言失败，之后排序候选；focused3/4记录测试装配API修正，未伪称生产缺陷。
[red_behavior.py](logs/task_100/red_behavior.py)刻意将操作优先级Low改为High，格式化后
运行同一行为测试：priority断言失败，0通过/1失败，Cargo exit101。脚本预期失败返回0，
finally逐字节恢复实现。focused5恢复后3通过，72案例通过。

## 最终验证

所有Cargo命令及调用Cargo的脚本通过[run.py](logs/task_100/run.py)原子mkdir动态领取
共享槽位1–10，记录计划/任务/会话/PID；失败/中断结束并等待子进程组后清理自有锁。
各阶段实际槽位均为 **1**，CARGO_TARGET_DIR为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，缓存保留。

各同名JSON保存精确命令/环境/退出码，log.gz保存完整合并stdout/stderr。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅本任务格式化差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_100 -- --nocapture | 0 | 3项通过，72条行为案例 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 150通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | workspace元数据通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过，仅既有cpu_usage弃用warning |
| CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked | 0 | 1699通过/0失败/1既有忽略，包含2项文档测试 |

baseline以 `sh -c` 顺序执行metadata、fmt check、workspace check及workspace test，
`&&`链整体exit0证明每项exit0。[counts.json](logs/task_100/counts.json)从完整日志
汇总测试数。[audit.json](logs/task_100/audit.json)逐项核对1103个Go/模块及只读plan文件
开始/结束哈希，含untracked扫描；无变化，Go/模块Git状态为空，git diff --check退出0。
文档本地链接解析通过，未引入Cargo.lock差异。各验证及使用槽位产物的进程均已结束，
自有slot-1.lock释放，缓存保留。没有必需验证缺口（采用用户明确授权的源码推导口径）。
提交只含本任务生产代码、独立测试、模块接线、说明及证据；编号任务状态/删除不提交。
