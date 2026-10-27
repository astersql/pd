# 任务122：调度器与检查器管理查询

前置121已核实：提交 `341f48d96`、[任务121证据](task-121.md)记录63场景、
4项聚焦测试、19项包测试及1791项workspace测试。实际 Handler、Coordinator、
scheduler controller、checker、diagnostic API 均可用。两份 plan.md 始终只读。

按用户后续明确授权，以只读Go源码/测试推导和Rust真实行为/故障验证替代缺失的
Go运行oracle。本任务没有运行Go行为oracle，也没有宣称运行差分。
[provenance](logs/task_122/provenance.json)记录source commit、checkout commit、
源码和fixture哈希、基准比对；Go输出/二进制为null。oracle-index的122条目仅是
source_assertion，不误作为运行证据。

只读复核调用方 `tests/server/api/checker_test.go::testGetStatus/testPauseOrResume`、
`scheduler_test.go::testPauseOrResume/checkEmptySchedulers`，覆盖暂停、到期、恢复、
空列表；未执行这些Go测试。HTTP对负delay的拒绝属于适配层，本任务直接领域
调用按checker_pause.go处理负TTL，不把它误称HTTP兼容测试。

生产API在 [scheduler_admin.rs](../../../pkg/schedule/handler/scheduler_admin.rs)：
`Handler::get_schedulers_controller/get_scheduler_names/get_scheduler_by_status`、
`get_checker_status/pause_or_resume_checker/get_diagnostic_result`，及接受可变
Coordinator的 `pause_or_resume_scheduler`。领域结果 `SchedulerQuery`、
`SchedulerPausedPeriod`、`CheckerStatus`；类型化 `SchedulerAdminError` 保留原错误。
[边界说明](../../../pkg/schedule/handler/scheduler_admin.rs.md)。不复制任何状态，
不添加HTTP/server依赖。时间为Unix秒，wire时间转换留给适配层；名称顺序升序，
这是Go未定义map顺序的确定性细化。其它字段、错误、期限不归一化或省略。

[fixture](fixtures/task_122.json)独立冻结6项过滤期望、两种scheduler、7种checker和
暂停输入/期限；其它字面期望在独立 `task_122_test.rs` 中，与实现分离。5项真实测试，
95项结果断言场景，全通过且无忽略：

| 测试 | 有效结果断言 | 覆盖 |
| --- | ---: | --- |
| task_122_behavior_cases | 15 | 控制器身份、GetNames、6过滤表行、时间段/到期、all、未知名错误和期限保留、0/负TTL恢复 |
| task_122_checker_states_and_errors | 50 | 7 checker各6种状态、3非法名称各查询/变更错误、错误后期限保留、溢出 |
| task_122_uninitialized_and_diagnostic_precedence | 10 | 7未初始化错误、3不支持诊断名称优先于bootstrap |
| task_122_diagnostic_real_recorder | 8 | 关闭诊断、缺scheduler、缺recorder、空结果、3真实记录状态/查询时间、暂停不伪造结果 |
| task_122_boundaries_and_metadata_preservation | 12 | 空过滤、空时间段、替换期限、scheduler/checker独立、恢复、溢出、region诊断入口、持久化不变 |

使用真实BasicCluster、Coordinator、MemoryStore、内置工厂、SchedulerController、
CheckerDispatcher及DiagnosticRecorder，仅时钟显式控制。注册scheduler真实写入
MemoryStore；所有管理动作前后对比revision和逐条Entry相同。管理动作本身没有I/O，
没有伪造存储故障。错误覆盖缺bootstrap、未知名称、关闭诊断、缺recorder及缺结果，
拒绝不改变既有期限。未知status按Go default返回enabled，不能错误地改为拒绝。

[initial](logs/task_122/initial.json)保留新增依赖使locked拒绝的早期失败；
[build](logs/task_122/build.json)、[first](logs/task_122/first.json)和
[expanded](logs/task_122/expanded.json)保留语法/API/测试装配失败，不冒充行为红灯。
离线重生成锁后撤回无关pulldown-cmark版本变化；最终Cargo.lock仅新增已有workspace
errs路径依赖。 [behavior-red](logs/task_122/behavior-red.json)故意反转disabled过滤，
同一fixture实际失败：返回leader，期望region，exit101，3通过/1失败。
正确条件已恢复。最终新增元数据比较使用既有Snapshot的revision和entries，不扩展存储API。

最终验证及只读复核结果见下文；原始命令输出、退出码、哈希保存在
[final.json](logs/task_122/final.json)。

| 最终命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-handler --locked --lib task_122 -- --nocapture` | 0 | 5通过、0失败、0忽略；95项结果断言 |
| `cargo test -p astersql-pd-schedule-handler --locked` | 0 | 24通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace准入 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace检查 |
| `cargo test --workspace --locked` | 0 | 1796通过、0失败、1项既有ignored；含2项compile-fail文档测试 |

各阶段均通过原子mkdir动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话标识/PID；Cargo及其子进程结束后已释放自己的锁并保留缓存。
[verify.py](logs/task_122/verify.py)记录实际执行及异常/中断时的等待、清理逻辑。
设置 `CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 GOFLAGS=-mod=readonly`；
既有etcd fixture编译产物位于槽位，proto仅只读消费固定模块ZIP。没有执行failpoint
启停、Go格式化、生成或模块整理。只有既有server deprecated字段warning。

[最终复核](logs/task_122/verification.json)：1101份Go/模块文件（含未跟踪路径扫描）
与起始清单逐一哈希和路径一致，Git无Go/模块差异，两份只读plan.md哈希不变。
文档相对链接有效，git diff --check和暂存检查通过。Cargo.lock仅新增errs路径依赖，
无远程依赖版本变化。提交范围仅handler实现/接线、独立测试、fixture、说明和本证据；
保留其它任务既有删除。

状态：已完成。
