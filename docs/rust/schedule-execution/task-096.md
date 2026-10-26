# 任务096：热点授予调度

状态：已完成。

依赖95实际提交 `50af912d2` 已核实：canonical hot_pending、hot_schedule等API和
[task-095](task-095.md)的5项聚焦、138项包级、1687项workspace运行证据存在。
两级plan与原始主题只读，仓库只有根AGENTS。开始时已有78、80–95编号任务删除，
属于其它任务变更，本任务不纳入提交。

按用户后续明确授权，以**只读Go源码/测试推导 + Rust真实行为和故障验证**验收。
没有Go运行oracle，没有声称运行差分。oracle-index不包含本任务可用运行输出。
[provenance](logs/task_096/provenance.json)保存来源commit、源码、相邻测试、init、
oracle-index和冻结[fixture](fixtures/task_096.json) SHA-256；runtime_oracle=null。
两个指定Go测试实际分别覆盖数值leader ID的BadRequest与leader-in限额耗尽/解除，
本次Rust真实存储、过滤器和操作器验证对应行为；未运行需failpoint的Go测试。

## 实现与边界

生产文件 [grant_hot.rs](../../../pkg/schedule/schedulers/grant_hot.rs) 实现
`GrantHotConfig`、`GrantHotScheduler::{schedule,dispatch,transfer,is_schedule_allowed,
encode_config,persist,update_config,reload}`，上下文复用真实BasicCluster、statistics
StoreLoadDetail、RuleManager、PlacementGuardContext、StoreStateFilter、operator
PeerOperatorFactory、StorageEndpoint及MemoryStore，不引入mock调度器或平行状态面。
[领域说明](../../../pkg/schedule/schedulers/grant_hot.rs.md)记录来源与宿主边界。

schedule抽样4类资源，独立选择leader/peer分支；dispatch当前byte降序、活跃region
去重、失败继续、最多1个operator。leader分支跳过配置内源，peer分支仅配置内非固定
leader源（忠实于Go实际代码）。只从配置目标选择，leader固定目标，peer排除现有副本。
复用普通placement safeguard（包括leader分支），HIGH state过滤及leader-in预算、
hot-evicted目标排除。操作器kind为LEADER或REGION|LEADER，priority HIGH，非HOT_REGION。

配置setStore保留顺序与重复并补leader；HTTP语义拒绝数值ID，CLI严格无符号十进制。
写失败保留新storeIDs但leader归零，持久化旧快照保持；reload读/解码失败不改内存。
HTTP路由、统计快照刷新、全局registry与operator提交由后续宿主/facade任务装配。
只新增lib模块注册与storelimit测试依赖；Cargo.lock只增加该workspace路径依赖，
离线生成曾升级pulldown-cmark，已恢复原版本和校验和，无远程依赖版本变化。

随机仅作为显式输入控制资源、分支、目标；时钟固定100纳秒，新store heartbeat亦100。
无字段归一化，不忽略目标、kind、错误、影响或命令差异。Go相等byte负载无map顺序
保证，Rust保留输入顺序；排序验证使用不等负载，不宣称相等负载选择的运行等价。

## 测试证据

3项独立真实测试，显式lib注册，无忽略：

- `task_096_behavior_cases`：18条冻结场景×2种conf-change模式=36条有效案例。
  覆盖leader/peer、源membership边界、缺少目标、无hot、活跃region、缺region、
  down/pending、目标pause、allocator错误、已有副本排除、leader-in耗尽、byte排序
  后错误fallback、随机多目标、目标无leader peer。另测缺source typed error，
  四资源正确采样及空资源、leader-in恢复为unlimited；验证真实operator步骤、
  priority/kind/目标、peer ID、confver、region影响量及classic/v2 heartbeat命令。
- `task_096_config_and_limits`：自动补leader、不重复追加、保留重复store、clone隔离；
  7条非法CLI输入；5条region与leader独立限额边界。
- `task_096_persistence_faults`：真实MemoryStore包裹仅I/O边界故障；3种非法更新
  不改配置，保存失败后归零、读取失败不改状态、恢复旧持久化配置、成功更新补leader、
  损坏JSON加载不改状态。

[red_behavior.py](logs/task_096/red_behavior.py)反转源membership条件，同一真实
behavior_cases得到0 != 1，0通过/1失败，Cargo退出101；外层确认预期失败退出0，
finally逐字节恢复生产文件。[日志](logs/task_096/red-behavior.log.gz)保留原始输出。
初始编译接线错误（OpKind API）保存在[first](logs/task_096/first.log.gz)，不算行为
红灯。补充heartbeat断言时修复Option<Operation>解包及classic/v2字段选择后，重新
运行最终聚焦、包级与workspace；没有降低行为断言或忽略测试。

## 最终验证

所有Cargo命令均经[run.py](logs/task_096/run.py)从槽位1–10原子mkdir动态领取，
记录计划/任务/会话/PID，异常时终止并等待子进程组后清理自有锁；缓存保留。
实际所有阶段均领取槽位 **1**；绝对CARGO_TARGET_DIR：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
每项命令、退出码和环境保存在同名JSON，完整stdout/stderr保存在log.gz。


| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅本任务Rust差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_096 -- --nocapture | 0 | 3通过/0失败/0忽略，36条表驱动行为案例 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 141通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过，仅既有cpu_usage弃用warning |
| CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked | 0 | 1690通过/0失败/1既有忽略，含2项文档测试 |

[最终计数](logs/task_096/counts.json)由[counts.py](logs/task_096/counts.py)解析完整日志；
[focused-final](logs/task_096/focused-final.log.gz)、[package](logs/task_096/package.log.gz)、
[workspace-test](logs/task_096/workspace-test.log.gz)保存最终运行输出。

[audit](logs/task_096/audit.json)核对开始/结束1119个Go/模块/plan文件的哈希，
含untracked扫描；SHA-256无变化、Go Git状态为空、git diff --check退出0。
[原始哈希集](logs/task_096/protected-hashes.json.gz)可审计。
所有验证及槽位产物进程已结束，自有slot-1.lock已释放，编译缓存保留。
未修改plan/主题/Go；其它编号任务已有删除不进入本提交。
