# 任务075：领导者驱逐调度

状态：已完成。批次74的提交 `5f0225c78`、收尾提交 `be49050e5` 及
[证据](task-074.md) 已核实，所需 canonical core/filter/operator/placement API 可用。
用户已明确授权整个计划采用 **只读Go源码/测试推导 + Rust真实行为和故障验证**。
本任务没有Go运行oracle或运行差分，绝不将以下源码推导描述为运行输出。

生产路径：[evict_leader.rs](../../../pkg/schedule/schedulers/evict_leader.rs)，
[边界说明](../../../pkg/schedule/schedulers/evict_leader.rs.md)。
公开 `EvictLeaderConfig::new/stores/batch`、`EvictLeaderScheduler::new/config/prepare/clean/is_schedule_allowed/schedule`、
`EvictLeaderContext` 和 `EvictLeaderResult`，由现有 lib.rs 局部注册、重导出。
没有修改Cargo依赖、features、全局注册工厂或运行时；配置事务留给批次76。

## 来源与冻结

来源基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[provenance](logs/task_075/provenance.json) 包含源码、测试、fixture、Rust文件的SHA256。
所核查Go文件和基准完全一致。现有 oracle-index.json 没有本任务要求的驱逐运行
输出；按用户授权以源码推导验收，Go输出及其hash明确为null。

[固定输入及推导期望](fixtures/task_075.json) 独立于待测Rust实现：

- evict_leader.go 的 scheduleEvictLeaderOnce/Batch：按配置store/ranges选择，
  优先健康region；第二次采样才允许不健康region，并排除down/pending目标；
  urgent target过滤、真实CreateTransferLeaderOperator、Urgent优先级。
- 同文件 uniqueAppendOperator 按region ID去重；整轮追加且严格 `len > batch`
  才停止，因此两store、batch3会产生4个操作，不能擅自截断为3个。
- PrepareConfig/CleanConfig仅控制transfer-in；prepare逐store尝试、部分成功
  保留、返回最后错误；clean需与一次prepare尝试配对，不是幂等调用。
- 相邻 TestEvictLeader 提供多目标和完成语义；TestEvictLeaderWithUnhealthyPeer
  提供pending/down排除；TestConfigClone提供配置快照；TestBatchEvict与
  TestEvictLeaderBatchLimit提供batch3/5/100及越界依据。只读审查这些测试，
  未运行其failpoint依赖链，也未新增Go harness。

确定性边界：固定时钟1000秒；store/target按ID排序处理Go未定义map顺序；
批量采样每10个draw固定一个region、健康优先场景轮转draw；shuffle为恒等。
检查完整目标集合及主目标属于该集合，不忽略操作、错误或优先级差异。

## Rust行为、故障与失败证据

`task_075_behavior_cases` 运行26/26固定案例：单store、未配置leader store、
空配置、范围包含/排除/部分相交/空ranges/多ranges、pending、down、两类不健康
目标同时存在、target transfer-in单个/全部暂停、source/target transfer-out暂停、
重复region去重、batch3/5/100、多store整轮超额、witness工厂失败、learner无目标、
缺失target、无region、优先健康region、不健康source可驱逐。
使用真实CoreCluster根/角色索引、StoreInfo、RuleManager及MemoryStore装配。
每个成功操作验证LEADER kind、Urgent、单步TransferLeader、完整目标集合、
零confver变化、source/primary leader_count影响量、两种heartbeat命令模式；
实际应用leader更新，再验证Operator成功及canonical区域索引。
备用目标只收取step cost而非leader_count，这一既有契约经源码核对后修正测试。

`task_075_config_and_prepare_faults`：batch0/101/usize::MAX拒绝；默认3、上限100
编码恢复及非法小数解码；配置clone快照；真实缺失store99触发StoreNotFound，
检查store1部分暂停与clean结果，独立transfer-in及transfer-out暂停均保留；
调度limit严格小于边界。无伪造调度结果，无新增忽略测试。
本任务不实现持久化写入，故不存在需要注入的MetaStore写入故障；其事务属于76。

[red-api](logs/task_075/red-api.log.gz)：API缺失编译失败，退出101。
[red-behavior](logs/task_075/red-behavior.log.gz)：临时绕过region去重，同一真实行为
测试在single-store断言实际3、期望1失败，1通过/1失败，退出101；已恢复正确实现。
[focused](logs/task_075/focused.log.gz)：最终2通过，26/26行为案例通过。
focused-influence-before-correction及before-fix日志保留测试装配中对备用目标
影响量的错误断言及一次未生效的文本修复；不是实现的预期失败证据。

## 验证与审计

每个命令的argv、退出码、Cargo环境及原始stdout/stderr位于
[logs/task_075](logs/task_075/)。Rust修改后先执行cargo fmt --all，再执行验证。
所有阶段原子mkdir动态领取仓库共享槽位，实际使用 **1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
run.py在成功、失败及中断后等待子进程退出，清理自己的owner与锁，保留缓存。

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_075 -- --nocapture` | 0 | 2通过，26/26固定行为案例及故障测试 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 35通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1584通过，0失败，1既有忽略 |

[审计](logs/task_075/audit.json) 核对1101个Go/模块文件清单与SHA256，包含untracked
扫描，Go状态和开始时一致；两份plan.md哈希保持一致。其它任务已有修改/删除不纳入提交。

[counts](logs/task_075/counts.json) 按实际test result行汇总（含2项文档测试）；
唯一忽略为既有tso_processes独立进程测试，无新增忽略。
日志按[说明](logs/task_075/log-encoding.json) 作保留原始字节的gzip压缩，不改输出。
最终共享槽位1锁已释放，缓存保留；文档链接、git diff --check和提交范围检查通过。
