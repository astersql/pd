# 任务110：调度器指标生命周期

依赖109的实际提交为 `acf6b2afa`；已审查[任务109证据](task-109.md)、canonical
诊断API、7个之前任务108运行生命周期测试及对应生产路径。任务109为5个聚焦、
184个包级、1733个workspace测试通过。本任务仅扩展canonical schedulers crate，
整体及包级plan始终只读。

按用户后续授权，本故事采用 **只读Go源码推导 + Rust真实行为及故障验证**。
未运行Go测试、未生成Go工具、没有Go运行oracle或运行差分声明。已有oracle-index
不提供完整指标生命周期运行输出。[provenance](logs/task_110/provenance.json)记录
当前及计划来源commit、源码SHA-256、冻结fixture哈希和null运行输出。

生产路径：[scheduler_metrics.rs](../../../pkg/schedule/schedulers/scheduler_metrics.rs)，
[边界说明](../../../pkg/schedule/schedulers/scheduler_metrics.rs.md)。新增依赖仅为
workspace既有prometheus，Cargo.lock仅新增本crate的路径依赖边；没有远程版本变化。
`SchedulerMetrics`提供宿主Registry注册、缓存事件句柄、真实规则/labeler计数、
热pending发布、按store删除、按scheduler移除和选择性Reset；`MetricsScheduler<S>`
包装canonical `LoopScheduler`，在实际进入policy时计数，在clean/drop时清理status。
不创建私有runtime，不把未admit的operator计为已提交事件；operator admission及
具体policy原因使用宿主边界持有的缓存事件句柄，不声称已替宿主自动装配所有策略。

[固定fixture](fixtures/task_110.json)记录20个Go指标族的name/help/type/labels、
13族Reset集合、10族store partial-delete集合，以及102个Go缓存变量合并后的
101个唯一预创建事件标签组合。witness另精确删除
`move-witness/{store}-in|out`；store1删除不能影响store10。事件counter、
HotPendingSum、histogram、tolerant、trend action/misc、range job不会被Reset。
Go grantLeaderCounterWithEvent使用grant-hot-region类型标签的既有别名被保留；
balance-witness与slow-trend使用实例name。所有表驱动断言共197项：20定义、20Reset、18缓存counter、18来源标签、
101预创建事件、8负载维度、10 partial-delete集合及2 witness方向（197通过/0失败；
另有故障、隔离、直方图和真实循环断言）。有效finite输入遵循来源，非法counter
增量、标签数、指标族、短/非finite负载返回Prometheus错误；短slice的Go panic
没有照搬。单批热负载发布不保证跨store事务；后检查以后的移除仍可能留有Go同样
的有界残余，须由宿主后续删除/周期清理。

测试独立注册于`task_110_test.rs`：

- `task_110_behavior_cases`：20指标族定义、20Reset成员案例。
- `task_110_store_cleanup_and_faults`：10 partial-delete族、两个witness方向、
  重复删除、保留store10、错误参数、重复注册与最后一个collector冲突回滚；
  撤销宿主冲突collector后能重新注册，证明未泄漏此前新增collector。
- `task_110_events_status_and_histogram`：18种counter句柄、跨Reset持续计数、
  pause/halt和scheduler移除、30桶边界/计数/sum、两个Registry独立。
- `task_110_eager_source_events`：首次调度前逐对核对101个唯一事件序列及零值。
- `task_110_source_event_labels`：18类来源标签规则，覆盖grant-leader别名与实例name。
- `task_110_real_loop_and_cleanup`：真实core、MemoryStore/RuleManager、witness策略；
  空队列10次真实尝试，真实witness事件产生一个操作器后共11次，取消不再增加，
  run/clean及捕获宿主边界panic后的drop删除status，counter仍保留。
- `task_110_real_counts_and_hot_pending`：真实规则与labeler计数，真实core/store
  snapshot的8个读写维度（含负load与CPU），短loads错误且不创建series；
  旧快照活着而canonical store已Removed、以及不存在store均不复活指标。
- `task_110_bury_during_publication`：真实canonical store在第一次read后buried，
  第一次read返回旧Arc；第二次read发现Removed，删除刚创建的8个pending序列，
  保留同store的其他指标。只控制read边界故障时序，无伪造调度结果。

初次`initial`为缺少API/依赖的编译红灯，不冒充行为失败。`green`记录新增既有
依赖后需刷新Cargo.lock的错误；离线锁刷新曾升级pulldown-cmark，已恢复其原版本
及校验和，仅保留本任务依赖边。`green2`修正测试proto数值getter误用，`expanded`
修正StorageEndpoint不可Clone和临时常量借用，均保留原始日志，不算生产行为缺陷。
`green3`3个测试通过，`expanded2`6个通过，`expanded3`7个通过。
第一次完整基线在`logs/task_110/before-eager/`中保存，7/191/1740测试通过；
随后审查补齐Go预创建的101个零值事件序列，加入独立测试并重新执行最终基线。

[行为变异](logs/task_110/behavior-mutations.json)分别移除预创建零值事件、Reset、反转store匹配、漏记真实
调度事件、移除buried后检查；5条Cargo命令均exit101并出现`test result: FAILED`、
真实断言panic，没有编译错误。脚本exit0，finally恢复源码。捕获故障的panic hook
文本是预期证据，不是被忽略测试。

最终命令/退出码/有效数量见[commands](logs/task_110/commands.json)，阶段槽位见
[final](logs/task_110/final.json)，原始输出为`logs/task_110/final-0.log.gz`至
`final-6.log.gz`，命令依次为：

```sh
cargo fmt --all
cargo test -p astersql-pd-schedule-schedulers --locked --lib task_110 -- --nocapture
cargo test -p astersql-pd-schedule-schedulers --locked
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

所有阶段从仓库根原子mkdir动态领取共享槽位，记录task/session/PID并继承绝对target。
实际使用槽位1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2`。正常、失败及中断均等待子进程结束后释放
自有锁，保留缓存；没有任务独立target或Cargo绕锁调用。

最终七条命令全部exit0：聚焦8通过/0失败，包级192通过/0失败，workspace
1741通过/0失败、既有ignored=1（含2个compile-fail文档测试）。全部197项表驱动
断言通过；五项错误行为变异均被真实断言识别。

[最终只读审计](logs/task_110/readonly-check.json)确认1101个Go/模块文件的清单、
SHA-256和Git状态一致，两份只读plan哈希一致，git diff --check退出0。
全workspace格式化未改动任务外Rust文件；相对Markdown链接已核实。
全部子进程退出，自己的槽位锁已释放，缓存保留。提交仅包含本任务实际代码、测试、
Cargo依赖边和证据；排除工作区原有的其他编号任务删除。
