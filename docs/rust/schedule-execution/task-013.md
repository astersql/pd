# 任务013：心跳流路由与替换

状态：已完成。依赖批次12提交 `e6a326a0b292073ad305c0d642d754413d4f29ca` 的
生产命令API、34组Go fixture、2项聚焦/包测试、1355项workspace测试证据已核实，见
[task-012](task-012.md)。本任务串行执行，两级plan和原主题保持只读。

## 生产路径与行为

[routing.rs](../../../pkg/schedule/hbstream/routing.rs) 提供 `StreamRouter`、
`RoutingHandle`、`HeartbeatStream` 和 `RouteEvent`。`StreamRouter::new` 借助
宿主共享 `Arc<StoresInfo>` 建立真实 Tokio 消费端，`step` 串行执行一个路由事件。
`bind` 入队不表示已生效；应用更新后旧流被替换。命令1024槽、绑定1槽，分别FIFO；
跨队列次序不作额外保证。无效目标删除绑定，未绑定跳过，发送失败删除旧流且不重试。
传输错误保留给适配层；消费者释放后生产者取得typed closed错误并保留命令。

发送等待期间不应用重绑；store移除不能撤回已发送命令，下次发送重新检查registry。
领域命令和传输隔离，不依赖proto/operator/server/runtime，不复制状态面、不创建
私有runtime。保活、取消/join与指标接续任务14；本任务不新增后台runner。
[领域说明](../../../pkg/schedule/hbstream/routing.rs.md) 记录具体契约与宿主调用边界。

## Go运行oracle

源码基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；三个实际来源Go文件与该commit
内容一致。[provenance](logs/task_013/provenance.json) 固定来源、依赖提交、既有
cluster.test/dlv哈希、接口itab符号和fixture SHA256。
[freeze.py](logs/task_013/freeze.py) 复用任务11/12审计的 TestSetOfflineStore 安全setup，
在第300行暂停，调用生产 `NewTestHeartbeatStreams`、`BindStream` 和 `SendMsg`。
真实router后台执行；既有 `mockhbstream.Send` 仅作为外部I/O边界，使用真实Go有界
channel。二进制现有itab构造合法的传输接口输入，其单channel结构体采用Go直接接口
表示；没有替换函数、改写Go或注入返回结果。nil传输channel触发原Send的一秒timeout。
等待使用既有nil-channel Recv计时，不改生产时钟。

[原始交互](logs/task_013/oracle.json)、[命令](logs/task_013/oracle.commands) 和
[进程记录](logs/task_013/freeze.out) 保留实际输出、退出0。关闭真实router后退出
隔离调试进程，不声称修改输入后的整个原Go测试通过。未编译Go、未启停failpoint，
来源二进制只读挂载；setup和执行路径无必需failpoint注入。

[fixture](fixtures/task_013.json) 的9组连续状态场景：未绑定、初绑、替换、缺失store、
Removed store、有序3条命令、Send超时删除、失败后跳过、重绑恢复。每组冻结两个
接收队列长度、当前绑定store IDs和该组实际响应JSON，共6条交付消息。Go运行还
读取到msgCh容量1024、streamCh容量1。归一化仅排除调试器地址与调度时间；队列
长度、绑定身份、错误导致的状态变化、完整消息字段与顺序均保留。路由逻辑在协议
分支外，本次classic运行不冒充额外nextgen运行；命令两种响应分支见任务12。

## 红绿验证与故障

[独立测试](../../../pkg/schedule/hbstream/task_013_test.rs) 先注册，取得
[缺少routing API的编译失败](logs/task_013/red-compile.out)，退出101；该错误不当作
行为断言证据。最小实现后取得 [first-pass](logs/task_013/first-pass.out)。

[mutation.py](logs/task_013/mutation.py) 逐一注入并在finally恢复7种错误：保留旧流、
固定错误store、忽略Removed、缺失store不删流、Send失败不删流、损坏payload、
错误队列容量。每次修改先cargo fmt --all；[结果](logs/task_013/mutation.json) 均
退出101并因同一组真实行为断言失败，没有用编译失败冒充。容量测试在初次变异检查
发现仅引用自身常量的弱点后，补充直接对照Go实际容量；最终7组均被断言捕获。

3项有效测试：`task_013_behavior_cases` 比较全部9组Go输出（9通过、0失败）；
`task_013_bounded_queue_order_and_errors` 用1024条真实队列消息证明容量和FIFO，
断开接收端后证明清除绑定、随后跳过、原传输错误保留与输入关闭；
`task_013_rebind_waits_for_inflight_send` 直接poll真实Tokio发送到Pending，证明
满传输阻塞时旧流完成后才能替换、绑定队列1槽背压、发送中移除store、跨store隔离。
额外Rust时序用例依据Go源码契约设计，不标为新增Go运行差分。没有本任务的持久化
操作，不引入假MetaStore或无关mock组件。

## 最终验证

[validation.py](logs/task_013/validation.py)、[结果](logs/task_013/validation.json)
保存确切argv、stdout/stderr、退出码和有效测试数，见下表。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-hbstream --locked --lib task_013 -- --nocapture` | 0 | 3通过，0失败/忽略；9组Go案例 |
| `cargo test -p astersql-pd-schedule-hbstream --locked` | 0 | 5通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | metadata |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace编译 |
| `cargo test --workspace --locked` | 0 | 1358通过，0失败，1既有忽略 |

[slot-run.py](logs/task_013/slot-run.py) 每阶段从1–10原子mkdir动态领取槽位，记录
plan/task/session/PID。所有阶段实际领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
Cargo、验证脚本及子进程均继承此绝对路径；正常/失败/信号路径等待子进程退出后
释放自己的锁，保留缓存。[最终阶段记录](logs/task_013/validation-stage.out) 保存结果。

[verify.py](logs/task_013/verify.py) 从原始Go交互重新解析9组队列、绑定和6条消息，
核对Go容量、来源哈希、依赖提交、变异断言、有效测试数和槽位释放；
[审计结果](logs/task_013/verification.json) 保存实际统计。
Go/模块及plan.md共1119个文件起止哈希一致，包含未跟踪源码检查，未产生Go差异。
Cargo.lock只增加已有Tokio的本地依赖边，无远程版本变化；`git diff --check`通过。
提交只包含任务13代码、测试、接线及证据，排除初始任务10/11/12文件删除。
完成后删除本编号任务文件，不改写plan或原主题。
