# 任务115：迁移协调器后台生命周期

依赖114提交 `42b89871d` 已核实：canonical `Coordinator::init_schedulers`、
初始化状态和配置恢复报告，以及20+36项恢复案例、1758项workspace通过证据。
本批次只有115，未启动并行任务。整体/包级plan始终只读。

依据用户后续明确授权，本任务使用**只读Go源码/测试推导 + Rust真实行为及故障验证**，
替代缺失的Go运行oracle/运行差分。没有运行Go调度测试或伪造其输出。
[provenance](logs/task_115/provenance.json)记录6份来源实际SHA-256和固定输入哈希，
Go运行command/output/binary字段为null。来源基准为
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`。
oracle-index已核实，其已有运行证据未覆盖本次生命周期验收。
`tests/server/cluster/cluster_test.go`与MCS scheduling server测试使用
changeCoordinatorTicker failpoint，按只读约束不裸跑、不注入。

生产路径：[coordinator_lifecycle.rs](../../../pkg/schedule/coordinator_lifecycle.rs)，
[领域边界与宿主契约](../../../pkg/schedule/coordinator_lifecycle.rs.md)。
`Coordinator::run` 先等待真实prepare checker，再调用114恢复，返回可驱动的
`CoordinatorRun`；`run_until_stop`直接驱动到停止。Rust借用式API要求宿主poll
`wait`，与Go立即启动goroutine的机制区别已明确记录。没有私有runtime或线程。

- 四个并发worker复用Patrol、SuspectRanges、ControllerDispatch和SlowTrend真实factory。
  suspect ranges与patrol消费同一个checker pending缓存；operator_handle只共享
  canonical admission的Arc，不复制状态。
- push每500ms处理真实operator/deadline，发现新增/替换identity，发送域心跳命令；
  终态结果进入records/success callbacks，非witness TTL及waiting promotion沿用已有API。
- slow-node每60秒检查existing/raft-kv2；已存在退出，非kv2不创建，创建失败不终止
  其它worker，下个周期重试；错误可从controller cleanup_errors观察。
- 调度策略执行仍由现有宿主具体ScheduleController负责，宿主通过scheduler_workers
  注册其run futures并使用传入的child token。协调器等待全部已注册策略及清理完成。
  未增加新的全策略adapter；动态策略执行仍使用已有宿主路径。
- patrol/routing/admission/scheduler错误取消其它worker并等待排空；重复stop安全。
  启动失败不会启动后台工作。必要依赖扩展clean_registered在join之后释放已Prepare
  的store控制状态，保留列表/持久配置；一个facade实例对应一个生命周期。

[固定fixture](fixtures/task_115.json)采用源码推导期望，4项表驱动：非raftkv2、
raftkv2、已有slow-trend/非kv2、一次MetaStore写失败后重试。固定输入与期望独立
于Rust实现；时间在显式宿主时钟边界推进，无输出字段/错误归一化。

6项显式注册测试使用真实BasicCluster、Dispatcher、RuleManager、MetaStore/MemoryStore、
operator、Patrol、SuspectRanges、scheduler factory和stream router。错误仅注入
MetaStore或关闭真实heartbeat I/O；宿主priority、split/scatter分发及LabelStatistics均调用现成实现；没有桩策略或只计mock调用。
Push身份发现使用BTreeMap，避免每500ms对全部operator作二次扫描。

1. task_115_behavior_cases：4项全部通过。真实TransferLeader region1从store1到2，
   经router/通道读取并核对目标peer；实际scan、共享pending的消费、slow列表及
   写失败重试结果均有状态断言。
2. task_115_stop_joins_inflight：在途patrol检查阻塞时取消两次，wait尚未结束且
   split-scatter尚未清理；放行检查后才完成清理并返回。
3. task_115_prepare_and_partial_startup_faults：准备阶段取消/预先取消不读取恢复配置；
   连续10次读取失败返回Restore错误、initialized=false且无patrol；随后grant
   配置写入失败、balance-leader成功，patrol仍运行并验证grant Prepare已回滚。
4. task_115_run_until_stop_joins_real_scheduler：Patrol无效间隔触发取消，真实
   EvictLoopScheduler/ScheduleController在相同child token上退出并完成Clean；
   store1 leader-transfer-in从paused恢复，证明等待具体策略清理。
5. task_115_routing_fault_stops_workers：关闭真实路由consumer，push返回Routing错误，
   Patrol清理完成，operator仍存在可供恢复重试。
6. task_115_repeated_stop_cleans_registered_policies：run.stop两次后wait正常完成，
   启动恢复的evict策略store控制已释放，canonical列表和持久配置仍存在。

红灯：[red-behavior](logs/task_115/red-behavior.json)，同一聚焦命令exit101，
0通过/2失败，测试断言协调器应正常启动但临时未实现路径返回Panicked；不是编译失败。
初次锁定依赖缺futures-util的exit101单独保留于red，不作为行为红灯。
额外[变异验证](logs/task_115/mutation.json)：临时将raft-kv2创建条件置false，原
behavior_cases在slow结果false/期望true断言失败，exit101，0通过/1失败；恢复生产
实现后再运行全部验证。没有保留变异生产代码。

最终命令、退出码、槽位见[final](logs/task_115/final.json)：

- cargo fmt --all
- cargo test -p astersql-pd-schedule --locked --lib task_115 -- --nocapture
- cargo test -p astersql-pd-schedule --locked
- cargo metadata --format-version 1 --no-deps
- cargo fmt --all -- --check
- cargo check --workspace --locked
- cargo test --workspace --locked

最终统计在[test-counts](logs/task_115/test-counts.json)。全部命令应exit0；聚焦6通过，
包级20通过；4项表驱动4通过/0失败。workspace结果按完整日志汇总，不以编译成功或
过滤后的零测试充当行为证据。

各阶段均通过原子mkdir依次动态领取共享槽位，实际均为1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
子进程继承CARGO_BUILD_JOBS=1、RUST_TEST_THREADS=2；最终workspace阶段还设置
GOFLAGS=-mod=readonly；没有绕过target-dir。
阶段结束后等待子进程结束，再删除自有owner文件并rmdir自有锁，保留编译缓存。
workspace既有etcd fixture harness仅编译scripts/rust-etcd-fixture只读源码，输出二进制、
WAL等位于领取槽位；没有failpoint注入或Go源码/模块改写。

[只读基线](logs/task_115/readonly-before.json.gz)冻结1101个Go/模块文件（含未跟踪
非忽略文件）哈希及初始空Git状态、两份plan哈希。
[最终只读检查](logs/task_115/readonly-check.json)核对文件清单、哈希、状态及plan
保持一致。git diff --check退出0；workspace格式化没有范围外生产文件改动。
其它任务原有的编号文件删除不属于本任务提交。

最终7条命令全部exit0：聚焦6通过/0失败，包级20通过/0失败；workspace
1764通过/0失败，1项既有ignored，含2项compile-fail文档测试。
全部槽位子进程已退出，自有槽位1锁已释放，缓存保留。Go/模块1101份清单、
内容哈希及Git状态与开始时一致，两份plan哈希完全一致，diff检查exit0。
