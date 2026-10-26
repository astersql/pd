# 任务067：均衡求解公共规则

状态：已完成，待回归验证（全量 workspace 被无关磁盘容量阻断）。任务文件保留。

依赖66由提交 `988d7b62e`、[task-066](task-066.md) 的证据及实际配置/存储 API
核实；没有并行任务或代理。实际实现为
[balance_solver.rs](../../../pkg/schedule/schedulers/balance_solver.rs)，领域边界和
API 见 [说明](../../../pkg/schedule/schedulers/balance_solver.rs.md)。

`BalanceSolver::new/from_cluster`、`tolerant_ratio/tolerant_resource/region_score_delta`、
`evaluate` 复用真实 core stores/regions、ScheduleConfig、BalanceSchedulerPlan 和
OpInfluence。每次计算两侧分数；`BalanceScores::should_balance` 严格比较分数，
计入在途影响、region 5倍放大和对称候选大小保护。缺失 source/target 返回
`BalanceError`。不涉及外部 I/O，因此故障覆盖是缺失资源、NaN拒绝及整数环绕，
没有伪造持久化故障测试。score公式继续使用既有core实现。

用户明确解除 Go 运行 oracle/差分准入；本任务是 **Go源码/测试推导 + Rust实测**，
没有 Go 运行输出或运行差分。[provenance](logs/task_067/provenance.json) 保存完整
来源commit、7个Go来源SHA256与fixture哈希；Go binary/output SHA256为null。
[oracle-index](oracle-index.json) 没有本任务运行案例，不借用其它任务的network证据。

只读核实 `TestRetryQuota` 是重试配额衰减/GC，`TestGetCountThreshold` 是placement
散射门槛，二者均非solver覆盖，不计入本任务通过数。实际契约还参考
`TestInfluenceAmp`、`TestTolerantRatio`、`TestShouldBalance` 以及
`TestBalanceRegionOrdinaryMoveNotBlockedByCandidateSize`、
`TestBalanceRegionLargeCandidateDoesNotOvershoot`，leader/witness相邻测试。
所有Go名称只表示查阅来源，没有运行这些Go测试。

冻结 [fixture](fixtures/task_067.json) 有20项手算预期，先于Rust实现写入，不从被测
实现生成；无字段归一化。覆盖leader count/size、region count/size、witness count/size、
默认/指定/自动ratio、截断、source/target在途符号、5倍放大、权重、阈值等值、
大候选、无候选。9项region案例另在v1高空间分支重放；v2使用capacity=0的size分支。
非零disk的压力公式由已有core测试覆盖，不声称这些20项fixture是新disk公式验证。

3项真实测试，均显式注册于canonical crate：

- `task_067_behavior_cases`：20项冻结案例全部通过，逐字段核对tolerance、delta、
  source score、target score、decision；另9次region v1重放。
- `task_067_missing_resources_and_nan`：缺失source/target的类型化错误、NaN拒绝、
  零在途量读取。
- `task_067_resource_selection_and_wrapping`：5项不同资源字段选择、i64::MIN环绕及
  真实v2负size回退、未知resource零分数，并确认在途量未被改写。

TDD：[red-api.log](logs/task_067/red-api.log) 为缺失API编译失败，退出101；
[red-behavior.log](logs/task_067/red-behavior.log) 是错误使用 `>=` 的同一行为测试，
`leader-equal decision` 实际true/期望false，1失败/1通过，退出101。
修正严格 `>` 后全部行为通过；后续增加资源字段/环绕边界。

验证命令和结果（各命令的JSON保存确切argv、退出码、环境、日志哈希）：

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_067 -- --nocapture` | 0 | 3通过、20冻结案例 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 13通过、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage deprecated警告 |
| `RUST_TEST_THREADS=2 CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 101 | 已执行79通过/1失败，cluster etcd夹具无法创建WAL |
| `RUST_TEST_THREADS=1 CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 101 | 已执行94通过/4失败，compat etcd夹具同因失败 |
| `git diff --check` | 0 | 通过 |

全量测试没有跑完，以上数量仅为实际已执行数量，见[counts.json](logs/task_067/counts.json)。
环境根因见[etcd日志](logs/task_067/etcd-disk-full.log)：`no space left on device`。
磁盘实测仅268MiB可用；串行重跑通过第一轮失败的全部46项cluster测试，但后续兼容
测试再次无法创建WAL。已清理本次阶段产生且进程已退出的4个临时fixture二进制，
见[cleanup](logs/task_067/cleanup.json)；不删其它任务缓存，不改fixture绕过失败。
待磁盘恢复后须重跑完整workspace命令，成功才删除任务文件。

所有Cargo操作由[run.py](logs/task_067/run.py) 原子mkdir动态领取共享槽位，本次实际
全部为槽位 **1**，绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
每次Rust编辑后先cargo fmt --all；阶段结束/失败/中断等待子进程退出再finally释放
自己的锁，保留编译缓存。本次锁已释放。

[audit](logs/task_067/audit.json) 核实1101个Go/模块文件清单、SHA256及Git状态不变，
两份plan.md内容不变。[before](logs/task_067/before.json) 保存起始快照。
Cargo.lock只增加3项已有workspace路径依赖，恢复离线锁生成引起的无关版本漂移；
无远程依赖版本变更。没有格式化引起的范围外Rust变动，已有其它任务删除不纳入提交。
日志仅规范行尾空白和EOF空行，JSON记录原始/保存SHA256。
