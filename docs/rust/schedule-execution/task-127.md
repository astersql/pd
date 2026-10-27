# 任务127：异步统计与 witness 事件桥接

前置126提交 `96465d0fd` 与 [task-126.md](task-126.md) 已核实：canonical handler
批量拆分、Coordinator、ControllerAdmission、SplitRetry 已实现；其聚焦5项、包级45项、
workspace1817项通过。本任务依赖的 HotCache/StatePipeline/TransferWitnessLeaderScheduler
均存在并通过本次真实消费及管线测试。整体和包级 plan.md 始终只读。

用户明确授权以只读Go源码/测试推导 + Rust真实行为/故障验证替代缺失的Go运行oracle。
`oracle-index.json` 的 HandleStatsAsync 是 task127/source_assertion，无冻结运行输出。
本任务未运行Go或failpoint命令，未声称Go运行差分。
[provenance](logs/task_127/provenance.json)记录来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、6份只读Go来源哈希与fixture哈希，
Go binary/output 均为null；来源文件与该commit字节相同。

源码推导：cluster.go::HandleStatsAsync先调用CheckRegionFlowAsync，再调用controller的
CheckTransferWitnessLeader；hot_cache.go依次投递write expiration/read expiration/write flow；
core/region.go::NeedTransferWitnessLeader只检查单独leader字段；scheduler_controller.go
只向已安装scheduler的channel非阻塞发送，满则丢弃；transfer_witness_leader.go消费原视图。
cluster_test.go只有两个benchmark及1/3/5 peers场景，没有证明双通道的行为断言；
server/cluster的同名benchmark也不提供本任务运行oracle，因此不把列出测试名当成覆盖。

生产路径为 [async_stats.rs](../../../pkg/cluster/async_stats.rs) 的
`handle_stats_async`，返回三个独立热点执行凭据与 `WitnessAdmission`。
[领域说明](../../../pkg/cluster/async_stats.rs.md)解释独立背压、停止和快照语义。
复用同一个 canonical HotCache 和已安装TransferWitnessLeaderScheduler的sender，
不创建第二份缓存或runtime。witness事件是输入Arc本身；重复heartbeat不去重。
`Collect` 才重读root，本入口不把排队快照替换成未来同ID版本。

[StatePipeline](../../../server/cluster/state_pipeline.rs) 的 `region_with_witness` 在原
submit_region_heartbeat成功后把同一receipt Arc传给桥接；`region`委托相同入口，传None
表示没有安装witness scheduler。宿主安装scheduler时提供recv_region_info()的sender，
移除和停止仍由宿主负责。过期报告不进入通道；已接受重复报告继续带流量。
队列失败不回滚root、不互相抑制。无持久化写入，未虚构存储故障。

[fixture](fixtures/task_127.json)在实现前冻结4行源码推导期望，不从Rust输出生成；
不归一化字段或错误。独立测试显式注册，无忽略测试。

| 测试 | 有效案例/观察 |
| --- | --- |
| task_127_behavior_cases | 4行：witness、普通leader、无leader、未安装；热点真实写入与原Arc身份 |
| task_127_backpressure_and_stop | 3行：两个队列满（保留两项expiration且write flow Full）、热点关闭仍投递witness、receiver断开Closed |
| task_127_duplicate_and_immutable_view | 1段：同ID重复两次保留原Arc，后续普通视图不替换既有事件，热点仍可读 |
| task_127_canonical_scheduler_consumes_event | 1段：真实scheduler消费生成region127单步urgent转移，实际命令指向store2，消费后为空且热点有统计 |
| task_127_witness_faults_do_not_suppress_hot_flow | 2行：witness Full/Closed均不抑制热点真实写入 |
| task_127_pipeline_same_view_duplicate_stale_and_stopped_hot | 1段：真实MemoryStore/StorageEndpoint/RuleManager/BasicCluster/runners管线，同Arc、重复、过期、更新root、热点停止仍转发且接受新root |

共6项行为测试、12个顶层案例/序列；只控制宿主时钟和外部队列容量/生命周期，
无内部算法mock。热点worker均显式shutdown，管线runners均stop并join。

初次[red](logs/task_127/red.json)为缺失API编译红灯，仅作接线记录；
[green](logs/task_127/green.json)首轮3项通过，[expanded](logs/task_127/expanded.json)
真实scheduler与管线也通过。离线更新锁文件带入的pulldown-cmark更新已恢复，最终
Cargo.lock仅新增本地cluster-utils包和server/cluster到它的依赖，远程版本均不变。

[behavior-red](logs/task_127/behavior-red.json)首次增强命令断言漏解包Option，exit101；
修复测试后第二次错误实现验证因同步recv等待缺失事件而中断，脚本终止并等待自有进程、
释放锁（[阶段记录](logs/task_127/behavior-red-fixed.json)只记录已完成的fmt，不能作行为红灯）。
所有本任务同步接收改为try_recv以使缺事件立即失败，然后
[behavior-red-bounded](logs/task_127/behavior-red-bounded.json)同一5项测试对“所有witness
都Full”的错误实现实际得到0通过/5失败，exit101；
[hot-red](logs/task_127/hot-red.json)对“热点全部Closed”的错误实现同样0通过/5失败。
正确实现已恢复，随后重新格式化并运行完整验证。

最终确切命令、退出码、测试摘要、槽位和压缩stdout/stderr哈希见
[final.json](logs/task_127/final.json)。[verify.py](logs/task_127/verify.py)每阶段原子mkdir
动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；
owner记录任务/计划/会话/PID。Cargo和子进程均继承绝对路径。正常、失败、中断路径
先等待/终止子进程，才清理自有锁，缓存保留。无新增features，GOFLAGS=-mod=readonly。

| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化通过 |
| `cargo test -p astersql-pd-cluster-utils --locked --lib task_127 -- --nocapture` | 0 | 5通过、0失败、0忽略 |
| `cargo test -p astersql-pd-cluster-utils --locked` | 0 | 5通过、0失败、0忽略 |
| `cargo test -p astersql-pd-cluster --locked --lib task_127 -- --nocapture` | 0 | 1通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace装配通过 |
| `cargo fmt --all -- --check` | 0 | 格式检查通过 |
| `cargo check --workspace --locked` | 0 | workspace检查通过 |
| `cargo test --workspace --locked` | 0 | 1823通过、0失败、1既有忽略 |

[汇总](logs/task_127/test-summary.json)从原始test result逐项计算，workspace包括2项既有
compile-fail文档测试，本任务未新增忽略；仅有既有proto deprecated字段warning。
[只读复核](logs/task_127/verification.json)由
[check_readonly.py](logs/task_127/check_readonly.py)逐路径比较起始清单：1101份Go/模块文件
（含未跟踪路径）和两份只读plan.md内容/路径完全不变，Go/模块Git状态为空。
`git diff --check`和文档相对链接检查通过。所有自有锁已释放，缓存保留。
提交仅包含本任务代码、测试、局部注册/路径依赖、说明与证据；其它任务删除保持原状。
状态：已完成。
