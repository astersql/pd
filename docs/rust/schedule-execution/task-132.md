# 任务132：迁移进度管理与过期清理

依赖131已核实：提交 `74c419508`、[task-131](task-131.md) 的真实测试证据及
`ProgressIndicator::{new,raise_target,adjust_window_length,push_at}` 可用。
实现为 [progress_manager.rs](../../../pkg/progress/progress_manager.rs)，由
[lib.rs](../../../pkg/progress/lib.rs) 导出 `Manager`，使用真实 core StoreInfo、
宿主 Registry、Tokio timer/CancellationToken。边界与 API 详见
[生产说明](../../../pkg/progress/progress_manager.rs.md)。无新 feature、无持久化或 I/O 模拟。

按用户后续明确授权，新增 oracle 使用只读 Go 源码/测试推导加 Rust 真实行为和故障
验证。**没有运行 Go 测试，没有伪造新 Go 输出或运行差分。** `TestProgress` 推导
增删、查询和 GC；`TestOnlineAndOffline` 推导两类进度与直接切换；
`TestDynamicPatrolRegionsDuration` 推导 manager 调用 getter 的时序与 19 分钟窗口。
`progress.go`/`metrics.go` 推导严格 TTL、平均 ETA overflow、Reset 保留 completed、
首次无指标、完成不更新指标及标签清理。

[provenance](logs/task_132/provenance.json)记录当前 source commit、主题原始基准、
四份只读 Go 来源、oracle-index、既有 native 原始事件和 fixture SHA-256。
[冻结 fixture](fixtures/task_132.json)在 manager 实现前写入：2组表驱动场景，5次更新
快照及2次完成快照；percent/speed/ETA 均按源码公式和已有 Go 测试推导。
浮点比较绝对/相对容差1e-10，不剔除任何状态、标签或错误。

`oracle-index.json` 的 `native_progress_gc` 是既有运行证据，原始记录为
[task_001/native-events.json](logs/task_001/native-events.json)，二进制 SHA-256
`446a9ce18ed5ac513d4cc66306cfe276db44cd2198db2d9e4e53d20520040523`。
观察到48.512秒的 removing progress/speed=0、ETA=max；63.533秒仍保留；
127.988秒 GC 后无 series；128.064秒 join exit=0。elapsed仅为观察时间，
不是精准 TTL 边界的运行证明；该档案只支持生命周期，未声称覆盖全部132场景。

测试 `task_132_test.rs` 显式注册，7项真实测试、0新增忽略：

1. `task_132_behavior_cases`：2组冻结 online/offline 场景及完成速度45、100/3。
2. `task_132_queries_targets_reset_and_switch`：不存在、None、两个 store、平均与
   overflow、查询副本隔离、目标增长不下降、直接 preparing→removing、Reset 后 GC、零间隔错误。
3. `task_132_gc_metrics_strict_ttl`：真实 Registry 3族指标、完整标签、完成保留原值、
   时钟回退、等于30秒不删、30秒+1ns删除、重复GC幂等。
4. `task_132_dynamic_getter`：121次更新、5次停顿，替换 getter 后4次停顿，
   速度每分钟1→0.5→10/19，ETA最终1672分钟。
5. `task_132_gc_cancel_join`：宿主 runtime、119秒无GC、120秒执行GC、取消并等待
   worker join、已取消 token 立即退出且保持完成队列。Tokio时间暂停只控制ticker；
   completed 使用明确注入的旧epoch时间，不假装墙钟同时推进。
6. `task_132_registration_failure_rolls_back`：真实 Registry 中故意预注册 speed，
   验证部分注册失败时撤销 progress 且保留宿主 speed，释放冲突后可正常构造。
7. `task_132_average_zero_and_repeat`：有限平均percent/speed/ETA、未知 action/state、
   两轮零目标完成重建、从未发布指标的 completed 清理。

失败—通过证据：

- [missing.json](logs/task_132/missing.json)：缺少 manager 模块，聚焦命令退出101。
  首次离线 generate-lockfile 仅为引入已有workspace依赖；自动升级的无关
  pulldown-cmark版本已恢复，最终lock仅增加本crate四项现有依赖。
- [red.json](logs/task_132/red.json)：临时把 `age > TTL` 改为 `>= TTL`，同一聚焦
  命令实际运行7项，6通过/1失败，TTL边界断言 `left: 1, right: 0`；finally恢复代码。
  这是真实行为失败而非编译/缺注册失败，故障实现没有提交。
- [final.json](logs/task_132/final.json)：恢复后全部必需命令输出、退出码、测试摘要与
  gzip日志哈希。每条日志保留合并stdout/stderr；无过滤错误或故障断言。

所有Cargo阶段经 [verify.py](logs/task_132/verify.py)原子mkdir动态领取共享槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话/PID；所有子进程继承设置、GOFLAGS=-mod=readonly，
正常/失败/中断时先等待或终止进程组，再清理自有锁，保留编译缓存。

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all` | 退出0 |
| `cargo test -p astersql-pd-progress --locked --lib task_132 -- --nocapture` | 退出0，7通过/0失败/0忽略 |
| `cargo test -p astersql-pd-progress --locked` | 退出0，14通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 退出0 |
| `cargo fmt --all -- --check` | 退出0 |
| `cargo check --workspace --locked` | 退出0 |
| `cargo test --workspace --locked` | 退出0，1841通过/0失败/1既有忽略 |

workspace包含2项既有compile-fail文档测试，无新增忽略，仅有既有proto deprecated
字段警告。全部子进程已退出，槽位1锁已释放，缓存保留。
`git diff --check`与本任务Markdown链接核对通过。状态：已完成。

[只读核对](logs/task_132/verification.json)对1101份Go/模块文件与2份plan.md比较哈希，
无变化、无新Go文件、Go Git状态为空。全局格式化差异仅涉及本任务Rust文件，
Cargo.lock无远程版本变化。整体和包级plan.md及原始主题均未修改。
