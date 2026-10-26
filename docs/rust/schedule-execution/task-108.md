# 任务108：调度循环与停止

依赖107核实为提交 `73ace03c9`，其 task-107 证据包含172个包测试、1721个workspace
测试和真实管理生命周期/存储故障验证。本任务遵循批次108串行执行，两级plan只读。
按用户后续授权，本任务使用只读Go源码推导 + Rust真实行为与故障验证，运行oracle
为null，没有运行Go命令、改写Go或伪称运行差分。来源、源码/fixture哈希见
[provenance](logs/task_108/provenance.json)。

生产路径为 [scheduler_loop.rs](../../../pkg/schedule/schedulers/scheduler_loop.rs)，
领域边界见 [实现说明](../../../pkg/schedule/schedulers/scheduler_loop.rs.md)。
公开API包括 `CacheCluster`、`LoopControl::{from_parent,stop,halt,set_delay_until,gate}`、
`LoopScheduler`、`ScheduleController::{schedule,allow_schedule,interval,set_interval,run,
start_local}`、`ScheduleTask::{stop,wait}`、`LoopExit`、`send_witness_event`，以及直接
调用canonical策略的 `WitnessLoopScheduler` 和 `EvictLoopScheduler`。
复用现有真实core/labeler/placement/operator和MemoryStore，无新增feature或远程版本。

## 行为与失败证据

Go只读来源：scheduler_controller.go的runScheduler、Schedule、AllowSchedule、
cacheCluster、Stop、Wait、CheckTransferWitnessLeader；operator_controller.go的
AddWaitingOperator，以及core/region.go的NeedTransferWitnessLeader。oracle-index
现有运行结果未覆盖本故事，按用户授权明确改用源码推导。fixture期望先固定为
严格TTL、10次重试、min10ms、空结果13ms、witness每次3条，不从待测实现反推。

初始装配出现Timestamp类型错误（initial exit101），补充测试出现store API拼写
错误（expanded exit101）；修正测试使用canonical API。expanded2出现eviction无操作
失败，查明fixture仅更新root未更新角色subtree，改用真实put_region后通过。
这些错误均保留原始日志，不冒充行为TDD。

新增推进验收断言先产生 [red-promotion](logs/task_108/red-promotion.log)：10ms tick
后活动操作器应为1，实际为0（exit101，1有效测试失败）。此前循环只入等待队列；
随后接入既有带限额的推进API，同一测试通过。行为变异脚本另外分别破坏重试上限、
退避、cleanup与label豁免，测试检出真实断言失败，finally恢复生产源码。
[red-results](logs/task_108/red-results.json)记录每个变异的确切命令和退出码。
首轮脚本将带自定义消息的assert!误判为非断言失败；日志保留，最终以panicked at
和FAILED识别运行断言，排除编译失败。

7个独立测试全部使用真实调度对象：

- task_108_behavior_cases：fixture四行gate（通过4、失败0），空重试、退避/上限、
  成功恢复min、4条witness按3+1返回、操作目标/步骤、普通heartbeat拒绝、取消保留间隔。
- task_108_cache_and_faults：store列表快照与单store实时查询、下一次刷新；region
  消失丢弃整批；坏事件类型化错误被保留、好事件继续；10000容量满队列、断连、非法interval。
- task_108_timer_stop_wait：9ms前不调度、10ms推进真实活动操作器、重复stop与wait；
  max_waiting=0在真实admission边界拒绝，记录错误并clean。
- task_108_label_and_retry_budget：真实MetaStore-backed labeler的deny、30条过期事件
  消耗10×3次尝试后保留下一条好事件、eviction的deny豁免和真实store暂停引用释放。
- task_108_inflight_cleanup_wait：仅延迟外部I/O边界；取消后wait保持pending、store
  仍暂停；释放I/O后完成clean、wait返回；父取消传播并clean。
- task_108_timer_backoff_and_gates：空结果后13ms重置，成功后10ms；halt与暂停期间
  不消费队列，恢复后真实活动操作器数从1增至2/3。

- task_108_panic_cleans_before_join：外部I/O边界panic，先有red-panic exit101
  的真实store引用残留断言；修复为捕获unwind、clean后恢复panic，宿主收到JoinError，
  store引用已释放。测试日志包含预期panic hook文本，但测试本身通过。

## 最终验证

最终运行、测试统计及只读审计记录在本目录logs/task_108下；所有Cargo阶段通过
run.py原子mkdir动态领取共享槽位，写owner.json并设置绝对CARGO_TARGET_DIR，
正常/失败/中断先等待进程退出再释放自有锁，保留缓存。

最终命令按顺序执行，全部退出0（完整命令/槽位见[commands](logs/task_108/commands.json)，
阶段结果见[final](logs/task_108/final.json)，原始输出见[final.log.gz](logs/task_108/final.log.gz)）：

```sh
cargo fmt --all
cargo test -p astersql-pd-schedule-schedulers --locked --lib task_108 -- --nocapture
cargo test -p astersql-pd-schedule-schedulers --locked
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

聚焦7通过/0失败，包179通过/0失败；workspace 1728通过/0失败，
既有ignored=1（含2个compile-fail文档测试）。
四行表驱动gate通过4/失败0；四项行为变异均被断言检出，变异Cargo命令exit101，
最终变异脚本exit0。推进和panic-clean两个先失败后修复的测试分别保留exit101证据。

所有阶段动态领取实际槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
最终阶段`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2`。全部子进程结束，自有槽位锁已释放，
缓存保留。没有使用任务私有target或绕过锁的Cargo调用。

[只读审计](logs/task_108/readonly-check.json)确认1101个Go/模块文件清单与哈希完全一致，
两份只读plan无变化，Go git状态无差异；git diff --check退出0。文档相对链接已核实。
审查Cargo.lock仅增加本包对workspace已有futures-util/tokio-util的依赖引用，无版本漂移。
提交仅包含本任务Rust、Cargo接线、fixture和证据，不包含既有其它任务的删除。
