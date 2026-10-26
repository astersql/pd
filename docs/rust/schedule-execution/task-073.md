# 任务073：见证副本均衡调度

状态：已完成。

前置批次72提交 `9c1a03c3d` 的真实核心/过滤/评分/操作API与
[证据](task-072.md) 已核对。本任务包级测试重新覆盖已有API。
两份 plan.md 和所有 Go/模块文件保持只读，不新增依赖或 crate。

生产实现：[balance_witness.rs](../../../pkg/schedule/schedulers/balance_witness.rs)，
[领域说明](../../../pkg/schedule/schedulers/balance_witness.rs.md)。
公开 `BalanceWitnessScheduler::schedule/is_schedule_allowed`、
`BalanceWitnessConfig::new/batch/ranges` 和 `WitnessBalanceContext`。
配置 batch 默认4、验证1–10；按 Go 当前控制流一次最多一个操作。
复用真实 canonical witness 索引、StoreState/SpecialUse、placement guard、
BalanceSolver 和 RangeOperatorFactory。宿主负责按 WITNESS 操作数调用限额准入。

## 来源和场景

依据用户后续授权，采用**只读Go源码/测试契约推导 + Rust行为实测**。
没有执行 Go，没有本任务 Go 输出或运行差分；没有把 oracle-index 的其它任务
运行条目当成本任务证据。[provenance](logs/task_073/provenance.json)
保存来源基准、当前提交、前置提交和来源/fixture SHA256；Go binary/output 哈希为 null。

Go `balance_witness.go` 的 Schedule/transferWitnessOut/createOperator 冻结来源排序、
目标角色限制、健康过滤、placement、评分、重试、单次操作数量和诊断阶段；
`balance_witness_test.go` 的 TestScheduleWithOpInfluence 与 TestTransferWitnessOut
提供 count/tolerance 和在途影响行为契约，未执行这些 Go 测试。
`utils.go` 定义 witness 按count的保守评分及整数化 tolerance；
`create_operator.go` 定义真实角色交换；`fit.go` 与 `filters.go` 定义位置保护。
RegionFit.Replace 按来源顺序优先允许同role交换，标签拒绝场景使用独立
leader/follower/witness-voter规则，不擅自改变该契约。

[fixture](fixtures/task_073.json) 冻结17个场景：正常、batch1、batch10多区域、
非witness不选、pending、down、范围外、均衡、在途影响、离线目标、learner目标、
witness目标、离线来源、placement排除leader、规则位置拒绝、无leader工厂错误、
普通共识角色交换。时钟固定1000秒，随机索引固定0，shuffle保持顺序；
固定唯一最低分目标store1、最高分来源store4。没有忽略操作差异或错误字段。

`task_073_behavior_cases`：17/17，逐条验证操作数量、kind WITNESS、MEDIUM优先级；
执行真实联合/普通操作步骤到 RegionInfo，检查成功终态、confver变化4、
peer ID/成员数/leader保持、来源取消witness和目标成为witness，随后更新canonical
区域索引并核对实际witness计数。检查双向 witness influence 为-1/+1，调用方快照不变。
无leader保留类型化 NoLeader 和step4 CREATE_OPERATOR_FAILED；当前重试10轮、
每轮3个目标，返回30个可审查错误，未注入模拟业务返回值。
规则通过真实 MemoryStore/StorageEndpoint 初始化与写入。

`task_073_config_and_limits`：默认值、合法1/10、非法0/11/usize::MAX、
二进制key JSON往返、非法batch反序列化拒绝、严格小于限额、零限额拒绝。

## 失败验证

[red-api](logs/task_073/red-api.log)：缺失模块/API编译失败，退出101；
也暴露初次测试装配的索引计数API不匹配，不算行为失败。
[first](logs/task_073/first.log)：补齐实现后发现计数API装配错误，随后使用
真实store_role_count接口修正。
[red-behavior](logs/task_073/red-behavior.log)：临时将评分准入替换为`if false`，
pending-influence产生1条操作（期望0），1通过/1失败，退出101；恢复后全部通过。
此突变证据说明同一行为测试能识别实际错误决策，最终源码不含突变。

## 验证记录

确切命令、退出码、环境及stdout/stderr位于 [logs/task_073](logs/task_073/)。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_073 -- --nocapture` | 0 | 2通过，17调度场景 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 31通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage废弃警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1580通过、0失败、1既有忽略 |

[计数](logs/task_073/counts.json) 从实际test result行汇总，包含doc tests；
零测试目标不作行为验收。既有忽略为tso_processes进程测试，未新增忽略。
每个Cargo阶段均通过run.py的原子mkdir动态领取共享槽位，实际为 **1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常/失败/中断均等待子进程结束后释放自己的锁，最终锁已释放，缓存保留。

[审计](logs/task_073/audit.json) 检查开始/结束1101个Go及模块文件SHA256一致，
含未跟踪文件扫描，Go Git状态为空；两份plan.md无差异。
`git diff --check`、文档相对链接及提交前staged diff检查通过。
提交仅含本任务模块、注册、测试、fixture、说明及证据；工作区已有其它任务删除
保留在提交之外。完成后删除自身编号任务文件，不改写只读指南或来源主题。
日志仅去掉末尾空行以满足diff检查，保留实际输出内容。
