# 任务 007：存储状态安全过滤

状态：已完成。批次 7，前置批次 6 提交 `3e51c6818`；已核验其
[证据](task-006.md)、真实候选/Filter API、3 项聚焦测试及 1332 项 workspace
测试证据。准入 oracle-index 仅将本行为列作待冻结源码契约；本任务补齐运行来源。
两级 plan.md 与来源主题任务只读。

## 实现与接口

[生产路径](../../../pkg/schedule/filter/store_state.rs)提供 StoreStateFilter、
`source/target`、`reason/filter_type`、AsyncFilter、SynchronousFilter 和
select_stores_async；[领域边界](../../../pkg/schedule/filter/store_state.rs.md)
说明条件顺序、原因身份和调用方式。复用真实 core/config/plan/storelimit，不新增
状态副本、feature、mock 预算或持久化路径。

已有同步 Filter 无法 await 真实 StoreInfo 限流查询。已按追加执行指示在本任务内
增加异步 companion 和显式同步借用适配：保留原同步 API，异步 selector 使用同一
真实 collector 并依次 await，不阻塞 executor、不新建 runtime、不将预算快照
冒充 admission。future 和可变原因实例不承诺 Send，适合宿主本地调度评估。
Cargo.toml/Cargo.lock 仅增加已存在 workspace 包 core-constant/storelimit 和测试
Tokio 边；offline lock 生成产生的无关 pulldown-cmark 更新已排除，没有远程版本漂移。

## Go oracle

[fixture](fixtures/task_007.json)、[provenance](logs/task_007/provenance.json)、
[来源哈希](logs/task_007/sources.json)固定源码 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`。复用任务 005 的 filter.test/dlv 和
任务 006 的 checker.test，经 SHA256 复核；没有编译、生成或修改任何 Go 文件。
TestMain 的 goleak/WaitForEtcdConnections、选中测试与初始化/销毁路径已审阅；
本次实际运行断言不需要 failpoint 注入。

[safe-tests](logs/task_007/safe-tests.json)完整运行 TestStoreStateFilter、
TestStoreStateFilterReason、TestSendStateFilter，3 项行为测试通过，退出 0。
TestSendStateFilter 是 snapshot sender 过滤，与本状态过滤不同，不能冒称覆盖
所有状态。前两个覆盖基本状态和 reason；注意 Reason 测试源代码两次都调用 Source，
不能用其名字声称 target reason 已验证。任务所列 DistinctScore/LabelConstraints/
RuleFit/RuleFitWithPlacementRule 属于其它过滤器，不当作本任务行为证明。
[checker-safe-test](logs/task_007/checker-safe-test.json)完整运行现有
ReplicaChecker/TestReplaceOfflinePeer，1 项行为测试通过（父 suite 不重复计数），
退出 0，真实调用 operator-builder 的 reject-leader 状态过滤。

[freeze.py](logs/task_007/freeze.py)在真实 Go Source 入口获取现有 conf/store/filter，
只调整显式领域输入，不替换算法、状态结果或断言。[commands](logs/task_007/oracle.commands)
和[原始输出](logs/task_007/oracle.json)记录 26 种状态×9 种选项=234 例。
选项为 leader、region、both、temporary-region、temporary-leader、urgent、scatter、
fast-failover、none；覆盖 removed/down/removing/disconnected/busy、双方向暂停、
slow/stopping/trend 驱逐、network-only/slow-score-only、snapshot sending/receiving
等于64和超过64、applying-only、pending 等于64和超过64、真实 add/remove/leader
预算耗尽及多条件先后次序。每例输入 Reason 显式清零，随后依次调用 source/target，
冻结两个 StatusCode/DetailedReason 与两个实际 Reason，包含 no-flags 的保留语义。

[reject-freeze.py](logs/task_007/reject-freeze.py)、[commands](logs/task_007/reject.commands)
和[原始输出](logs/task_007/reject.json)使用现有 checker 的 noleader=true store
及匹配的 reject-leader 配置，再观察9组选项。共243例、486次实际返回；期望由 Go
transcript 提取，不读 Rust。API probes 调整了输入后退出，不宣称改变后的完整 Go
测试通过；未修改的完整测试通过证据单独保存。

归一化只从 fixture 去掉调试指针/栈和日志时间，保留全部状态码、详细原因、filter
reason、输入字段与次序。宿主显式时钟下重放 Go 健康（future heartbeat）、down
（heartbeat=0）与 disconnected（观察时间减5分钟）类别；它们远离严格20秒/30分钟
边界。精确等号与1ns越界是额外 Rust 源码契约测试，未冒称它们具备纳秒级 Go
运行差分。来源与 fixture 哈希在审计报告中核验。

## 测试驱动与故障

[red-compile.out](logs/task_007/red-compile.out)保留先注册测试而缺少
StoreStateFilter 的退出101；零行为测试，仅作缺失 API 证据。首个运行发现 fixture
复用 Go Reason、Rust 每例新建的不等价输入；随后冻结脚本为每例显式清零，保留
初次失败事实，不改变生产的原因保留语义。

[mutation.py](logs/task_007/mutation.py)在同一已注册聚焦测试上引入并还原真实错误，
每次先 cargo fmt；[结果](logs/task_007/mutation.json)证明差分测试识别错误行为：

| 变异 | 通过/失败 | 退出码 |
| --- | --- | ---: |
| 绕过 removed | 1/4 | 101 |
| snapshot `>` 改为 `>=` | 4/1 | 101 |
| 绕过 leader 入站预算 | 3/2 | 101 |

最终5项测试：task_007_behavior_cases（243组/486返回），
task_007_clock_budget_and_boundaries（显式时钟等号/1ns、未来heartbeat、pending=0与
uint64→int64、预算查询不预留、四种优先级/temporary、reason保留、v2、空limiter
提前短路），task_007_async_chain_and_diagnostics（两种过滤器顺序×3种选择方向、
真实 collector 首失败、Arc身份、重复/空输入和空链），以及 nil limiter 和
collector资源类型错误两项真实 panic 故障。后两者是领域编程错误，不是模拟外部
I/O 故障；本任务不访问 MetaStore，不构造无关持久化故障。

## 最终验证

精确命令、退出码和测试计数保存在
[validation.json](logs/task_007/validation.json)，对应 validation-0.out 至
validation-6.out 为完整 stdout/stderr。

[slot-run.py](logs/task_007/slot-run.py)各阶段按 mkdir 原子动态领取仓库共享槽位，
记录 plan/task/session/PID；Cargo及子进程继承绝对 CARGO_TARGET_DIR，正常、失败
及 INT/TERM 均等待子进程终止后仅释放自有锁，保留缓存。本次实际槽位和最终
结果见 validation.out 与最终审计。


| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-filter --locked --lib task_007 -- --nocapture` | 0 | 5通过/0失败/0忽略 |
| `cargo test -p astersql-pd-schedule-filter --locked` | 0 | 11通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据检查 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace编译 |
| `cargo test --workspace --locked` | 0 | 1337通过/0失败/1忽略 |

唯一忽略项为既有 `tso_processes`，没有新增忽略。既有 proto 弃用警告未导致失败。
所有 Cargo 阶段实际动态领取槽位1，使用
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，阶段结束
自有锁已释放，缓存保留。命令与槽位由 [validation.out](logs/task_007/validation.out)
记录。

[verify.py](logs/task_007/verify.py)重新从 Go transcripts 核对243组原始输出，
校验冻结文件/来源/二进制哈希、全部 Go/模块文件起止内容（含 untracked/ignored）、
17份只读 plan.md、生产依赖方向、远程依赖版本、测试及变异计数与 Markdown 链接。
见 [Go审计](logs/task_007/go-verification.json)及
[证据审计](logs/task_007/evidence-verification.json)。`git diff --check` 和暂存
差异检查均通过。只提交本任务实现、接线、测试和证据；完成的编号任务文件按约定
删除，两级 plan.md、来源主题任务和 Go 文件保持原样。
