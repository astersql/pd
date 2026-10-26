# 任务066：调度配置原子持久化

状态：已完成。

依赖65由提交 `58234409f`、[task-065](task-065.md)、已通过的5项既有测试
和真实 BaseScheduler/StorageEndpoint API 核实；批次66仅此任务，没有并行代理。
两个 plan.md 只读。实际生产路径为
[config.rs](../../../pkg/schedule/schedulers/config.rs)，边界见
[config.rs.md](../../../pkg/schedule/schedulers/config.rs.md)。

`SchedulerConfig<T>` 提供 `get/save/reload/update/update_json`，编码并持久化成功
后才发布候选值；任何编码、解码或 I/O 失败保留本地旧值，load 不会部分覆盖。
完整替换和字段校验由具体 schema 承担；严格 schema 拒绝未知字段，未知名称的
load 返回 Decode。单实例通过可变借用串行化；跨实例保持原存储 last-write-wins，
不添加 CAS 或迁移格式。DefaultSchedulerConfig 从 base.rs 原样迁入本文件，
保留65的禁用失败语义及公开 API。

用户已解除Go运行oracle准入限制。证据是**只读Go源码/测试推导和实际Rust行为测试**，
没有Go运行输出或运行差分。[provenance](logs/task_066/provenance.json)记录来源commit、
6个来源SHA256、与计划基准逐文件相同比对及fixture SHA256；Go binary/output哈希为null。
既有 oracle-index.json 没有任务66的运行案例，不将80/81的network案例冒充本任务证据。

Go入口核实：`TestSchedulerConfig` 覆盖save/load和另一个名称加载失败；
`TestDefaultSchedulerConfig` 覆盖初始false、持久化true、重新实例化读回disabled、
不刷新其它本地字段与显式load全量字段。`TestReloadSchedulerConfigWithoutSchedulerDoesNotPanic`
是controller中空handler错误，非本配置API；`TestShuffleLeader`、`TestRejectLeader`、
`TestRemoveRejectLeader`、`TestShuffleHotRegionScheduleBalance` 分别验证shuffle策略、
label/balance/evict过滤、删除store配置和placement开关下hot shuffle，不是通用配置原子
发布测试。以上均只读查阅，没有运行，也没有把名称计入通过数。

重要语义边界：Go config.go base save/load 没有发布管理；default setDisable 先修改
本地值、保存失败不回滚，65已验证此行为，本任务保留。Go evict-leader applyStoreIDs
在保存失败时回滚；balance-leader update 保存失败仍保留参数。任务66显式要求的新
原子发布API不能泛称为所有Go更新路径的等价迁移。具体策略HTTP更新留给后续任务。

5项有效测试（0忽略），均使用真实StorageEndpoint和MemoryStore，仅MetaStore I/O可注入：

- `task_066_behavior_cases`：执行[fixture](fixtures/task_066.json)的3个冻结输入/预期，
  每项覆盖save/load/update、重新实例化恢复，另验证未知名称错误。期望独立于被测实现，
  覆盖普通文本、HTML字符、空字符串和Unicode，无字段归一化。
- `task_066_failures_retain_old_value`：写失败/读失败保持旧本地值，确认持久化旧值；
  5类坏输入（空、截断、null、错误类型、未知字段）分别验证update与reload不变更状态，
  最终重试成功并读回真实新值。
- `task_066_cancel_before_commit`：I/O边界暂停真实写入，确认存储仍为旧值；drop Future
  后本地保持旧值，重试成功。没有后台任务/计时或mock业务逻辑。远端提交后响应丢失仍
  可能导致存储与本地短暂不同，必须reload对账，不宣称分布式事务或自动安全重试。
- `task_066_encode_failure`：真实Serialize错误保持本地值与已保存false字节。
- `task_066_default_disable_restores`：缺失disabled为false，保存true，重新实例化恢复，
  保留已存其它字段，再保存false并通过另一个实例读取。

TDD证据：`red-api.log` 最初缺失模块/API，退出101，仅作接线记录；
[red-behavior.log](logs/task_066/red-behavior.log)用同一组最终测试检测“先发布再保存”的
错误实现，3失败/2通过（写失败后错误暴露new、编码失败错误发布、取消仍保留候选），
退出101。修复为编码→保存→赋值后 [focused.log](logs/task_066/focused.log) 5通过、
[package.log](logs/task_066/package.log) 10通过，均退出0。

所有Cargo命令通过 [run.py](logs/task_066/run.py) 原子mkdir动态领取共享槽位，
记录任务/计划/会话/PID，所有子进程继承绝对CARGO_TARGET_DIR。中断先终止并等待
进程组，再finally只清理自己的owner和锁目录，保留缓存。命令参数、退出码、槽位、
路径保存在同名JSON。每次Rust实现编辑后先cargo fmt --all；全workspace格式化
没有范围外Rust改动。

最终验证全部退出0：

| 命令 | 有效结果 |
| --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_066 -- --nocapture` | 5通过、3项fixture、0失败/忽略 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 10通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过 |
| `RUST_TEST_THREADS=2 CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 1559通过、0失败、1既有忽略 |
| `git diff --check` | 通过 |

[counts.json](logs/task_066/counts.json)根据实际test result行汇总；workspace含2项
既有compile-fail doctest，不将0项doctest当作有效测试。仅有既有
`pkg/server/core_region_adapter.rs:134 cpu_usage` deprecated警告。
日志只清理行尾空白和EOF空行；各命令JSON保存原始与保存日志SHA256。

本次所有阶段实际动态取得槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
全部子进程已退出，自己的锁已释放，编译缓存保留。[audit.json](logs/task_066/audit.json)
确认1101个Go/模块文件清单、内容哈希和Git状态不变，两个只读plan.md不变，
DefaultSchedulerConfig逐字搬移；全量验证结束后再次核对通过。
Markdown的14个本地链接均存在。没有Cargo依赖或lock变动，只有本任务Rust/测试/证据
和直接关联说明变更；已有其它编号任务删除不纳入本任务提交。
