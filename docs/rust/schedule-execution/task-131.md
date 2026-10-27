# 任务131：巡逻周期动态窗口调整

依赖130已核实：提交 `3d07881fa`，其[证据](task-130.md)记录3项包测试和
1830项workspace测试通过。已有 `ProgressIndicator::adjust_window_length` 实现
本故事所需行为，任务131复用[生产实现](../../../pkg/progress/indicator.rs)，不复制
`indicator_window.rs`，不改Cargo依赖、feature、Go或plan.md。公开API继续由
[lib.rs](../../../pkg/progress/lib.rs)导出，语义见[领域说明](../../../pkg/progress/indicator.rs.md)。
本次仅增加聚焦测试、注册和验证文档。

按后续明确授权，使用只读Go源码/测试推导与Rust真实行为/故障验证替代缺失运行
oracle。已核实 `oracle-index.json`，其中没有本任务动态窗口的冻结运行输出。
**没有执行Go测试、没有Go二进制/输出哈希，不声称Go/Rust运行差分。**
[provenance](logs/task_131/provenance.json)保存当前来源commit、4份只读Go来源及
[fixture](fixtures/task_131.json)的SHA-256。来源基准为
`3d07881fa`（主题原基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`）。
期望来自源码公式/相邻测试，在Rust测试运行前冻结，未从待测实现输出反推。

来源和范围：

- `indicator.go` 的 `adjustWindowLength`：clamp到600–7200秒后整除采样间隔+1，
  初始窗口与调整后窗口有一个端点样本差异；过大采样间隔在构造时限制为600秒。
- `TestAdjustWindowLength`：覆盖下限、上限、内区间；本次进一步覆盖599/600/601秒、
  7199/7200/7201秒、0秒、半秒采样、7秒非整除采样、600/601秒interval。
- `TestWindowCapacity` / `TestMoveWindow`：active front可前移及后退，缩窗不丢弃
  inactive历史；窗口上限121，history容量也为121，但可用样本不足时只取已有样本。
- `TestDynamicPatrolRegionsDuration`：累计删除量0→120，停顿5步，扩为19分钟
  再停顿4步，恢复10步；速度按每分钟归一为1→0.5→10/19，ETA按分钟为
  880→1760→1672→1653。本次继续缩为10分钟、扩到2小时，front依次121、21，
  验证扩窗能重用曾被缩窗排除的历史；fixture的期望和原始输入均保留。
- `TestProgressIndicatorUpdateProgress`：完成/回退仍按原始样本差算速度；
  `TestProgress`、`TestOnlineAndOffline`只作manager调用顺序和完成语义参考，
  未声称本任务迁移manager状态机/GC/指标；`progress.go`中removing分支在push前
  调整窗口，Rust本任务直接以累计工作量调用真实indicator。
- `TestMain`仅安装goleak，不是数值oracle。含failpoint的Go测试没有裸跑或注入。

测试在独立 `pkg/progress/task_131_test.rs` 显式注册，共4项：

1. `task_131_behavior_cases`：12组冻结边界，断言length/capacity、调整的延迟
   生效、首次有效样本速度/percent/未完成时间。浮点误差为相对/绝对1e-12。
2. `task_131_dynamic_continuity_reuses_retained_history`：6个冻结检查点及10步
   恢复，每步断言真实front、速度，检查点同时断言percent、ETA、容量和状态。
3. `task_131_insufficient_samples_and_completion`：窗口13但只有3个样本仍用
   两个时间间隔算速度；完成后调整不改快照/时间，后续零速完成保留Go的max ETA，
   回退时速度clamp为零，percent回退且完成时间保留。
4. `task_131_invalid_interval_and_zero_target`：零interval返回typed
   `InvalidUpdateInterval`（Go会除零panic），零目标在最大窗口下仍完成且速度/ETA为0。

没有模拟领域对象或外部I/O；无持久化/网络故障边界。时间仅通过 `push_at` 注入
UNIX epoch+0/1/2/3秒，不归一化错误或状态。Rust `Duration::MAX`验证上界clamp，
Rust Duration不表示负值；Go的负duration等价落入下界，非新增Rust输入类型。

现有生产算法在新增测试首次运行时已通过，见[baseline](logs/task_131/baseline.json)。
为了验证测试能识别错误行为，临时将Rust `adjust_window_length` 的 `+1` 改为 `+0`：
[red](logs/task_131/red.json)记录聚焦命令退出101、4项真实行为断言全部失败
（600≠601、10≠11、12≠13、120≠121）。随后在finally恢复生产文件，
生产 `indicator.rs` 与依赖提交字节相同；不是缺API/缺注册编译失败，也没有提交故障实现。

全部Cargo阶段通过[verify.py](logs/task_131/verify.py)原子mkdir动态领取共享槽位1，
绝对 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁owner记录计划/任务/会话/PID；Cargo及其子进程继承同一target目录与
`GOFLAGS=-mod=readonly`。正常、失败及中断路径都等待/终止进程组后清理自有锁，
保留缓存；测试阶段不执行任何Go写入或failpoint命令。

最终命令与完整stdout/stderr、退出码、有效测试数见[final.json](logs/task_131/final.json)，
日志以gzip保存并在manifest记录哈希。默认features、无新增忽略测试。

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all` | 退出0 |
| `cargo test -p astersql-pd-progress --locked --lib task_131 -- --nocapture` | 退出0，4通过/0失败/0忽略 |
| `cargo test -p astersql-pd-progress --locked` | 退出0，7通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 退出0 |
| `cargo fmt --all -- --check` | 退出0 |
| `cargo check --workspace --locked` | 退出0 |
| `cargo test --workspace --locked` | 退出0，1834通过/0失败/1既有忽略 |

workspace结果包含2项既有compile-fail文档测试，无新增忽略。仅有既有proto deprecated
字段警告。[只读核对](logs/task_131/verification.json)确认1101份Go/模块文件和2份
plan.md路径/内容及Go Git状态未变；最终Rust来源哈希见
[final-rust-hashes.json](logs/task_131/final-rust-hashes.json)。
全部子进程已结束，自有槽位锁已释放，缓存保留。`git diff --check`和本任务文档链接
检查通过。提交不包含其他任务已有删除；编号任务文件在完成报告后删除。

状态：已完成。
