# 任务068：范围视图与重试配额

状态：已完成。

用户续执行授权确认前置67实现/API及聚焦3项测试可用，无关workspace回归不得阻止
本任务实现；Go运行oracle准入已经解除。依赖实现提交 `28f809a17`，本次scheduler
包级测试再次覆盖67。两份plan.md只读，不改写主题来源。

生产实现：[range_view.rs](../../../pkg/schedule/schedulers/range_view.rs)，
[边界/API说明](../../../pkg/schedule/schedulers/range_view.rs.md)。
`RangeCluster` 复用真实 core 根与角色索引，投影store空间/计数、局部leader/follower
随机选择及平均大小；`BalanceSolver::from_range` 接入范围容忍比率。
`get_count_threshold` 复用真实RuleManager、排他标签约束和拓扑权重。
`RetryQuota` 实现按ID登记、衰减、重置及GC，`get_key_ranges` 保留原始字节并返回
带参数/字节偏移的类型化解码错误。没有新增任务、私有运行时或I/O路径。

## 来源与固定期望

本任务是 **只读Go源码/测试推导 + Rust行为实测**，没有本任务调度行为的Go运行输出或运行差分。
[provenance](logs/task_068/provenance.json) 保存实际来源commit、Go源文件SHA256、
fixture SHA256；Go binary/output SHA256显式为null。
[oracle-index](oracle-index.json) 已核实，不借用无关运行案例冒充本任务证据。

`range_cluster.go` 定义半开范围扫描（相交region计入完整数据），原store压缩比、
先截断MiB再乘单位、Available无符号环绕，以及仅局部重载的查询。
`utils.go` 的 getKeyRanges/retryQuota/getCountThreshold 是对应算法来源。
`TestRetryQuota` 覆盖10→5→2→1、GC和reset；`TestGetCountThreshold` 在z1的三台
store间分配leader阈值，验证正阈值store数为3，不是rangeCluster的运行覆盖。
这些Go测试仅查阅，没有执行。

[fixture](fixtures/task_068.json) 的32项冻结预期由上述源码手算：6项配额序列、
11项query-unescape案例、15项角色/引擎阈值。无字段归一化；仅阈值浮点比较允许
绝对误差1e-10。范围输入 `[b,d)` 包含 `[a,c)` 大小10和 `[c,e)` 大小20；
原store大小1000、使用100MiB，放大系数10，所以局部大小30、使用3MiB、剩余197MiB。
规则场景第一段 `[a,c)`：leader均分三台z1，follower独占无engine的z2，learner独占
显式engine=tiflash；第二段 `[c,e)` 的voter仅匹配rack=r1的store1。
因此store1 leader/peer阈值为4/3，store2/3为1/3，store4 peer为1，store5 learner/peer为1。
测试通过真实MemoryStore装配RuleManager，并使用优先级覆盖组保留范围外默认规则。

## 实测

独立注册的3项测试：

- `task_068_behavior_cases`：6项配额序列、11项解码案例，以及GC重复ID/空集、
  reset、首次attenuate、错误参数与偏移；包含非法百分号、非UTF-8、NUL、加号、
  奇数参数、反向范围及全范围。
- `task_068_range_projection`：真实3-region集群，逐字段核对局部leader/region/
  pending及大小/空间；独立raw stats保持父集群不变；learner/witness索引与原store
  计数保留；v1分数由1000重算为30；范围solver覆盖/回退；leader/follower抽样；
  全局根读仍为3；缺失store、空范围、零使用量Arc、MiB截断和Available下溢。
- `task_068_count_threshold_roles_engines_and_splits`：15项冻结角色/引擎阈值，
  另5项边界：匹配集为空只清零当前段、Removing/Removed排除、空store集、
  禁用规则时leader数量及peer拓扑份额。

没有新增I/O，因此故障验证针对解码、缺失对象、无匹配及数值边界；不虚构持久化
故障或Go运行证据。仅抽样随机输入通过真实core已有回调控制。

TDD：[red-api](logs/task_068/red-api.log) 为缺失API编译失败，退出101（同时暴露并修正
两处测试装配API误用）。[red-behavior](logs/task_068/red-behavior.log) 将配额衰减除数
错误设为3，同一冻结测试实际3/期望5，1失败/2通过，退出101；恢复除数2后通过。
日志first-green、threshold-green中的失败分别是空范围及默认规则装配的错误预期，
已依真实source/core/placement契约修正，不作为产品缺陷或通过证据。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_068 -- --nocapture` | 0 | 3通过，32冻结案例及上述边界 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 16通过，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage废弃警告 |

每条命令的确切argv、退出码、槽位及环境记录在对应JSON；stdout/stderr在同名log。
全部Cargo操作通过[run.py](logs/task_068/run.py) 原子mkdir动态领取槽位，本次实际
为槽位 **1**，绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
每次Rust修改后先执行cargo fmt --all；正常/失败/中断均等待子进程结束后finally释放
自己的锁。该脚本还仅回收本阶段新生成、测试持有PID已经结束的临时etcd夹具二进制，
保留编译缓存。

开始实现时磁盘一度不能创建文本文件或锁目录。在确认无活动Cargo/rustc/etcd夹具后，
核实历史临时fixture文件的持有PID不存在，回收约9GiB临时二进制；遇PID探测权限错误
即停止，保留对应文件。没有删除Cargo编译缓存或活跃任务的锁/进程。


最终全仓回归：`RUST_TEST_THREADS=2 CARGO_BUILD_JOBS=1 cargo test --workspace --locked`
退出 **0**，实际 **1565通过、0失败、1既有忽略**（`tso_processes` 需要另行装配
Go/Rust进程二进制，既有测试默认忽略；本任务没有新增忽略）。
见[完整日志](logs/task_068/workspace-two-threads.log)、[计数](logs/task_068/counts.json)。

先前单线程全仓退出101，实际1542通过/1失败/1忽略；TSO重启测试子进程退出成功，
但stdout标记解析失败。改用两个测试线程后，该定向重启测试及同一workspace目标
全部通过，没有修改TSO或syncutil源码，不把先前失败隐藏成通过。

最终[audit](logs/task_068/audit.json)核实1101个Go/模块文件清单、SHA256和Git状态
完全不变，两份只读plan.md内容不变。Cargo.lock仅增加已有workspace placement路径
包的直接依赖，没有远程版本变化。所有自己的槽位锁已释放，编译缓存保留。
日志仅规范行尾空白和EOF空行，各命令JSON保留规范前后SHA256；业务fixture不做
该规范化。已有其它任务文件删除未纳入提交。`git diff --check`与暂存差异检查通过。
