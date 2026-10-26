# 任务065：调度器基础契约与间隔

状态：已完成。

前置64由提交 `3916a4950`、[task-064](task-064.md) 和真实 splitter API 核实；
该提交包含4项聚焦测试、13项包测试与workspace成功证据。本任务仅65，无并行代理。

生产路径：[base.rs](../../../pkg/schedule/schedulers/base.rs)，领域边界与兼容限制见
[base.rs.md](../../../pkg/schedule/schedulers/base.rs.md)。新canonical crate
`astersql-pd-schedule-schedulers` 已加入workspace；没有注册尚未迁移的18类策略。
`Scheduler<C>` 复用真实SchedulerCluster、Operator、BalanceSchedulerPlan，提供基础
生命周期及配置默认方法，具体策略必须实现调度与准入判定。Base提供名称、类型、
间隔、null配置、默认类型及异步禁用状态。HTTP装配不在本任务范围。

用户已解除Go运行oracle准入限制。本证据为**只读Go源码/测试推导与实际Rust行为验证**，
没有Go运行输出或运行差分。[provenance](logs/task_065/provenance.json)保存来源commit、
5个Go来源SHA256、与计划基准逐文件相同比对、fixture SHA256；Go binary/output哈希
明确为null。oracle-index既有运行案例不覆盖65，未以其冒充本任务证据。

只读Go入口核实：

- `TestReloadSchedulerConfigWithoutSchedulerDoesNotPanic` 覆盖controller中空handler错误，
  不覆盖Base reload或通用codec；本任务没有迁移该controller。
- `TestShuffleLeader` 覆盖shuffle leader策略、操作kind与限流，属于后续策略任务。
- `TestRejectLeader` 覆盖label、balance和evict策略的拒绝leader与目标过滤。
- `TestRemoveRejectLeader` 覆盖移除已删除store的evict配置。
- `TestShuffleHotRegionScheduleBalance` 覆盖placement开关下hot shuffle策略。

以上测试均只读查阅，没有运行，也没有把它们的名字计为验证通过。
本任务期望直接冻结自 `base_scheduler.go`、`scheduler.go`、`config.go` 与 `types/type.go`。

[fixture](fixtures/task_065.json)保存11条原始间隔输入/预期与5条JSON编码输入/预期；
由测试执行，不从被测实现生成。间隔以signed ns精确比较，没有时间归一化。
指数增长1.3后截断、5s截顶；线性加3s；零增长不截顶，覆盖0、负值、整纳秒舍入
与上限两侧。JSON比较字节编码/字段语义；map顺序规范化为排序，payload数字按
Go any转float64，包含2^53之外舍入。长度不足显式返回领域错误，替代Go越界panic。
支持范围及非有限浮点/无效UTF-8/任意动态类型限制见领域说明。

5项有效测试：

- `task_065_behavior_cases`：执行全部11条interval fixture。
- `task_065_codec_and_payload`：5条编码fixture，5类坏JSON、类型错误、真实serde
  serializer错误、延迟decoder成败、payload坏项跳过/同名覆盖/null/长度/空输入、
  嵌套大整数转float64。
- `task_065_base_default_and_disable`：基础名称回退/覆盖、type、min/next间隔、null
  配置、reload；18个类型在Classic与NextGen中的身份；非default config禁用无效；
  MemoryStore上实际保存disabled并保留其它字段、外部值刷新、缺失/坏JSON/null/类型错
  保留本地状态。
- `task_065_unregistered_and_unsupported`：18类与未知类型全部创建/decoder查找失败；
  空表名称查找与scatter历史别名；基础update显式Unsupported。
- `task_065_persistence_failure_retains_local_state`：真实StorageEndpoint/MemoryStore，
  仅MetaStore I/O边界注入Unavailable；写失败返回Storage错误、本地禁用状态不回滚、
  读失败保留本地状态、恢复后检查原持久化数据并刷新本地值。

没有mock业务判定，无私有runtime或后台任务；不宣称完成任何具体调度策略。

TDD证据：

- [red.log](logs/task_065/red.log)：初次编译MemoryStore构造函数接线错误，退出101，
  只算接线记录。
- [red-behavior.log](logs/task_065/red-behavior.log)：遗漏指数增长的实现触发真实断言，
  10ms得到10ms而非13ms；同一fixture测试与base测试2失败/2通过，退出101。
- `fmt.log`/`focused.log`保存追加I/O故障测试时的import位置语法错误，修正后
  `focused-final.log`与`focused-complete.log`均5通过；后者还覆盖延迟decoder与payload舍入。
- 最终行为及workspace命令结果见下方，未将0项doctest算作有效测试。

所有Cargo命令经 [run.py](logs/task_065/run.py) 原子mkdir动态领取共享槽位。
owner记录计划/任务/会话/PID；子进程继承绝对CARGO_TARGET_DIR；中断先终止并等待
子进程组，finally只释放自己的锁、保留缓存。日志同名JSON记录确切参数、退出码、
slot与target路径。使用槽位 **1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
每次Rust编辑后先运行cargo fmt --all。

最终验证（全部退出0）：

| 命令 | 有效结果 |
| --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_065 -- --nocapture` | 5通过，11条间隔+5条编码fixture |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 5通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过 |
| `RUST_TEST_THREADS=2 CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 1554通过，0失败，1既有忽略 |
| `git diff --check` | 通过 |

最终日志：`focused-contract.log`、`package-final.log`、`metadata.log`、
`fmt-check-final.log`、`check-final.log`、`workspace-final.log`。
[counts.json](logs/task_065/counts.json)汇总实际测试数，含2项既有compile-fail doctest。
仅有既有 `pkg/server/core_region_adapter.rs:134 cpu_usage` deprecated警告。
所有子进程已结束，自己的槽位1锁已释放，缓存保留。
日志仅规范行尾空白和EOF空行，原始输出与保存日志SHA256均在各命令JSON内记录。

首次workspace全量运行 `workspace.log` 退出101：storage-endpoint中
`store_meta_etcd` 与 `timestamp_values_etcd` 的既有独立etcd进程报
`panic: failed to create WAL`。当时磁盘剩约2GiB；没有修改既有fixture或Go源码。
重跑显式设置 `RUST_TEST_THREADS=2 CARGO_BUILD_JOBS=1` 降低同时启动的fixture数量；
确切环境及最终退出码保存在 `workspace-final.json`。


离线生成lock时发现无关pulldown-cmark版本漂移，已恢复原版本与checksum；最终lock
仅增加本地scheduler包，无远程依赖升级。全workspace格式化没有范围外Rust差异。
[审计](logs/task_065/audit.json)逐项核对1101个Go/模块文件及两个只读plan.md，内容、
文件清单与Go Git状态均不变。其它任务已有删除不纳入本任务提交。
