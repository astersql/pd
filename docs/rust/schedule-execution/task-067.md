# 任务067：均衡求解公共规则

状态：已完成。2026-10-27在最新共享工作区完成延后回归；下列早期磁盘失败记录保留，
最终通过证据见文末。任务文件由本会话按完成规则删除。

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
当时待磁盘恢复后须重跑完整workspace命令；该项已由文末最终回归满足。

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


## 2026-10-27 延后回归完成

先阅读并核实[任务141](task-141.md)的Etcd复用/恢复办法与
[任务156](task-156.md)的中立存储拆分、严格依赖审计和最终通过记录。
来源语义仍按用户授权采用只读Go源码/测试推导加Rust真实行为/故障验证，
没有新增Go运行oracle或运行差分。后续任务增加的`from_range`也纳入本次包级测试。
未修改生产Rust、测试、Cargo配置或任何Go文件，原先错误已确认是外部容量问题。

按用户指定顺序等待任务58的Cargo阶段及子进程全部结束、锁释放后，才动态领取槽位。
[串行核验](logs/task_067/regression/serialization.json)记录任务58全量通过记录的哈希，
[上下文核验](logs/task_067/regression/context.json)记录156的11条最终命令日志哈希核对。

所有命令由[回归runner](logs/task_067/regression/verify.py)执行，原子mkdir动态领取
槽位 **1**，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
`CARGO_BUILD_JOBS=4`、`RUST_TEST_THREADS=2`。从已核验源fixture建立槽内硬链接；
[Go拦截器](logs/task_067/regression/reuse-go/go)拒绝实际Go构建/测试，仅复用已验证二进制
和只读kvproto ZIP，避免重复编译及物理复制。二进制SHA256为
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`，
[调用日志](logs/task_067/regression/go-reuse.jsonl)保留全部请求。

[最终命令记录](logs/task_067/regression/final.json)保存确切命令、退出码、实际槽位、
压缩stdout/stderr路径、SHA256和全部libtest汇总行：

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_067 -- --nocapture` | 0 | 3通过、0失败/忽略；20冻结案例及9次region v1重放 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 192通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage deprecated警告 |
| `cargo test --workspace --locked` | 0 | 1925通过、0失败、1既有忽略 |
| `python3 docs/rust/schedule-execution/logs/task_067/regression/audit.py` | 0 | 日志、只读文件、链接、输入及锁审计通过 |
| `git diff --check` | 0 | 通过 |

全仓测试包含2项既有compile-fail doctest；0测试target不计入有效测试数。
[最终审计](logs/task_067/regression/audit.json)及
[脚本](logs/task_067/regression/audit.py)核对1101个Go/模块文件（含ignored/untracked）的
清单、内容SHA256和Git状态不变（扫描包含ignored/untracked，排除target/.git/node_modules生成缓存），
两份plan.md不变，本次Rust/Cargo验证输入不变。
所有Cargo及槽位产物子进程已结束，自己的锁已释放，缓存保留。无需业务修复；
只提交本任务回归脚本和稳定证据，排除其它任务改动；编号任务删除不单独提交。
