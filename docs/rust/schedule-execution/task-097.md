# 任务097：标签领导者调度

状态：已完成。

批次96真实依赖已满足：提交 `f68ab2147`，canonical grant_hot 与共享 core/filter/
operator/placement API 存在，[task-096](task-096.md)保留3项聚焦、141项包级、1690项
workspace通过证据。仅根AGENTS适用。开始时已有78、80–96编号任务删除，与本任务
无关，不纳入提交。整体与包级plan、原始主题保持只读。

用户后续明确授权允许以**只读Go源码/测试推导 + Rust真实行为和故障验证**验收。
本任务没有Go运行oracle，也没有声称Go/Rust运行差分。oracle-index核实无本任务
可复用运行输出。[provenance](logs/task_097/provenance.json)保存source_commit、
源码、已有测试、oracle-index与冻结[fixture](fixtures/task_097.json)的SHA-256，
runtime_oracle=null。未运行Go测试或failpoint注入。

## 实现

[生产实现](../../../pkg/schedule/schedulers/label.rs)提供 `LabelConfig::{new,from_args,
ranges}` 和 `LabelScheduler::{new,config,encode_config,is_schedule_allowed,schedule}`。
只追加lib模块/测试注册；复用现有依赖，无Cargo.toml/Cargo.lock变更。
[领域说明](../../../pkg/schedule/schedulers/label.rs.md)记录边界。

真实BasicCluster、StoreStateFilter、RuleManager、PeerOperatorFactory与operator共用
既有状态。每个拒绝leader源抽样一个leader region，排除down/pending follower，
MEDIUM过滤目标，随机选一个并创建单步LEADER操作器，MEDIUM优先级，空备选目标。
无目标继续下个源，builder失败结束本轮并返回typed error，最多一个operator。
不检查源transfer-out暂停，保留Go行为。限额以count < limit供宿主admission调用。
配置复用规范hex range JSON与query-unescape CLI解析，保留null/空范围语义。
不增加Go TODO尚未提供的reload。HTTP、registry、指标发布与operator提交留给宿主。

Go map顺序不确定，Rust排序ID后接受显式随机输入；只稳定不保证的顺序，不缩减目标
集合。固定时钟1000000000000纳秒。未归一化或忽略目标、kind、错误、影响或命令字段。
配置无新增持久化I/O；placement测试使用真实MemoryStore/StorageEndpoint。

## 测试与失败证据

独立 `task_097_test.rs` 显式注册2项测试，无忽略：

- `task_097_behavior_cases`：20条冻结源码推导场景 × classic/v2 heartbeat = 40条
  有效案例。覆盖reject/no reject、无region、down/pending、全部不健康、拒绝目标、
  pause/no target、源pause-out、range miss/touch/partial、learner/witness、断联、
  leader-in耗尽、placement deny/allow与随机最后一个目标。检查typed LeaderNotAllowed、
  目标、空备选集、description、kind、MEDIUM、单步、confver=0、leader_count影响、
  两种heartbeat命令以及执行后operator成功状态。
- `task_097_config_and_limits`：hex编码/解码、CLI默认与尾参数、query-unescape错误、
  3种损坏JSON、5项严格限额边界（含0与u64::MAX）。

初始缺API/模块编译失败退出101保存在red-initial，不视作行为红灯。修复测试接线
StoreInfo/Arc与desc方法后，首次行为运行暴露witness期望错误：Go GetFollowers包含
witness，builder拒绝选中的witness，故修正fixture为0 operator + LeaderNotAllowed；
没有修改生产行为来迁就该期望。focused-first/second/third保存过程输出。

[red_behavior.py](logs/task_097/red_behavior.py)反转生产reject-label匹配条件，同一
真实behavior_cases在首条reject场景得到0 != 1，0通过/1失败，Cargo退出101；
外层确认预期失败退出0。finally逐字节恢复生产文件，随后最终聚焦与包级验证通过。
日志保留原始stdout/stderr、退出码与命令环境。

## 最终验证

所有Cargo入口经 [run.py](logs/task_097/run.py)以原子mkdir动态领取仓库共享槽位
1–10，写入计划/任务/会话/PID，设置绝对CARGO_TARGET_DIR；异常时终止并等待子
进程组后清理自有锁。各阶段实际均为槽位 **1**：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

所有命令、环境与退出码保存于同名JSON，stdout/stderr合并保留在log.gz。

| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅本任务Rust差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_097 -- --nocapture | 0 | 2通过，40条表驱动行为案例 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 143通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过，仅既有cpu_usage弃用warning |
| CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked | 0 | 1692通过/0失败/1既有忽略，含2项文档测试 |

[counts.json](logs/task_097/counts.json)由完整日志解析计数。
[audit.json](logs/task_097/audit.json)核对1119个Go/模块/plan文件的开始/结束SHA-256，
包含untracked扫描；无差异、Go Git状态为空、git diff --check退出0。所有文档相对
链接均解析到存在的文件。仅新增label实现、说明、独立测试、fixture、证据与局部lib接线。
未改Go、模块、两个plan或原主题。所有验证及槽位产物进程已退出，自有槽位锁释放，
保留编译缓存。无剩余必需验证或oracle缺口（以用户授权的源码推导验收口径）。
