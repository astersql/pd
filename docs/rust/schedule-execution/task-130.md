# 任务130：进度数值与速度窗口

前置129已核实提交 `238d27829` 和 [task-129.md](task-129.md)：最新区域统计
收集及真实heartbeat桥接已通过，workspace 1827项通过。任务130不需要调用其业务
API，依赖为线性准入；progress 领域算法无 core/transport/runtime 依赖。

采用用户后续明确授权的 Go 只读源码/测试推导 + Rust 真实行为及故障验证。
`oracle-index.json` 无本行为冻结 Go 运行输出。本任务未运行 Go、不声称 Go/Rust
运行差分。当前只读来源 commit 为 `238d27829`，原主题基准为
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`；四份源码及 fixture SHA-256 见
[provenance](logs/task_130/provenance.json)。没有 Go binary/output 哈希。
固定 [fixture](fixtures/task_130.json) 的输入、期望在实现前从源码推导，非 Rust 输出反推。
无归一化；JSON 的 `"max"` 表示 f64::MAX；完成时间在时钟边界注入 epoch+7秒。

来源覆盖：

- `TestProgressIndicatorUpdateProgress`：0→10→50→120，percent .1/.5/1，speed 10/25/40。
- `TestWindowCapacity` / `TestMoveWindow`：保留容量独立于 active window，front 随
  push 前进，缩窗/扩窗复用历史；Rust以分钟采样覆盖121容量、11/20/31/121 active长度。
- `TestAdjustWindowLength`：低于下界、超过上界、30m、10m、2h共5个duration。
- `TestProgress` / `TestOnlineAndOffline`：累计 online/offline 数值与完成时间；
  offline 累计删除量由 manager 转换，本任务仅验证转换后的数值，manager/GC后续迁移。
- `TestDynamicPatrolRegionsDuration`：120次增长、5次停顿、扩为19m、4次停顿、
  10次恢复；期望归一采样单位速度1→.5→10/19，ETA 880→1760→1672；
  front=110，随后逐次+1，history始终≤121。
- `TestMain` 只安装 goleak，不是数值行为案例；未运行含failpoint的Go manager/GC测试。

生产文件 [indicator.rs](../../../pkg/progress/indicator.rs) 实现 `Progress`、
`ProgressIndicator::{new,push,push_at,adjust_window_length,raise_target}`，
由 `pkg/progress/lib.rs` 导出；[领域说明](../../../pkg/progress/indicator.rs.md)。
初始percent保持Go的current/total（零总量可能NaN/Inf）；首次零总量push完成。
负速度归零；超额样本先算速度再截断percent；非有限ETA变为最大浮点值。
完成且零速度仍为max ETA；完成时间允许刷新，目标升高不清除已有时间。
零间隔Go会除零panic，Rust返回typed error，领域库不panic；过大间隔限制为10m。
用VecDeque保存历史及active sample count，替代链表front，保留调整延迟到下一push的语义。

显式注册3项测试：

1. `task_130_behavior_cases`：11组fixture序列、19个逐样本percent/speed/ETA断言，
   包括零total、零速度、负速度/负进度、超额完成、非零初始工作、半秒间隔、过大间隔。
2. `task_130_window_capacity_front_and_dynamic_patrol`：上述完整动态巡检序列及
   5个duration边界，断言真实保留历史、front、速度、ETA和调整的延迟生效。
3. `task_130_completion_target_and_invalid_interval`：零间隔错误、目标只增、完成
   时间保留/刷新、单样本active window的完成零速度max ETA。

[red.json](logs/task_130/red.json) 保存首次真实失败，update_progress 空实现时
3项均失败，退出101；关键断言分别为0!=.1、0!=.12、0!=.75。
[green.json](logs/task_130/green.json) 保存最小实现后3项通过；其中stdout案例数
曾误写21，最终修正为19，fixture始终11组/19个样本。原始stdout/stderr合并压缩保存。
离线generate-lockfile仅保留新路径包变更；其尝试升级pulldown-cmark的差异已排除，
所有远程依赖版本与原锁一致。没有新增feature；serde_json仅为测试依赖。

最终验证（全部退出0；每条命令、槽位、日志哈希见 [final.json](logs/task_130/final.json)）：

| 命令 | 有效测试/结果 |
| --- | --- |
| `cargo fmt --all` | 通过 |
| `cargo test -p astersql-pd-progress --locked --lib task_130 -- --nocapture` | 3通过、0失败、0忽略 |
| `cargo test -p astersql-pd-progress --locked` | 3通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过 |
| `cargo test --workspace --locked` | 1830通过、0失败、1既有忽略 |

workspace统计包含2项既有compile-fail文档测试；本任务未新增忽略。
只有既有proto deprecated字段警告。全部阶段动态领取槽位1，绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[verify.py](logs/task_130/verify.py) 用原子mkdir领取共享锁，写入plan/task/session/PID，
所有Cargo与子进程继承绝对路径与GOFLAGS=-mod=readonly；中断/失败均等待或终止
进程组后释放自有锁。最终子进程已全部结束、自有锁已释放，缓存保留。

[只读核对](logs/task_130/verification.json) 确认1101份Go/模块文件及两份plan.md
路径/内容未变，Go/模块Git状态为空；差异检查与文档链接检查通过。
[最终Rust哈希](logs/task_130/final-rust-hashes.json) 固定通过版本。
提交排除其它任务已有删除；任务文件在完成证据报告后删除。

状态：已完成。
