# 任务042：副本数量与位置修复检查

状态：已完成。

实际生产路径：[replica_shape.rs](../../../pkg/schedule/checker/replica_shape.rs)，
入口 `ReplicaShapeChecker::check(ReplicaShapeContext, region, allocate)`，
[领域说明](../../../pkg/schedule/checker/replica_shape.rs.md)，
[行为测试](../../../pkg/schedule/checker/task_042_test.rs)。
仅注册 canonical checker 模块/测试，未改 Cargo 依赖或 Cargo.lock。

## 依赖与来源

批次41提交 `a9bf41cb9` 和 [task-041](task-041.md) 提供已验证的
ReplicaFaultContext/Outcome、PauseController、ReplicaStrategy 和真实
PeerOperatorFactory 链路；已核实实际实现及32场景/包级/workspace证据。
本次形状检查供宿主在异常修复未返回操作后调用；不承担 controller 装配。

已审阅 oracle-index.json，其 checker 源码清单不能充当运行输出。
按用户后续授权，本任务使用只读 Go 源码/测试推导期望，**没有运行 Go，
不声称 Go/Rust 运行差分**。fixture 标记 `go_execution=false`，保留来源 commit
和来源文件 SHA256：[task_042.json](fixtures/task_042.json)。
Go 契约为 replica_checker.go 的 Check/checkMakeUpReplica/
checkRemoveExtraReplica/checkLocationReplacement，以及相邻 TestBasic、
TestDistinctScore、TestDistinctScore2、TestStorageThreshold、TestOpts 和
TestRemoveExtraReplicaKeepsUnhealthySource。步骤/leader/kind 依据
create_operator.go、builder.go、kind.go；错误使用 canonical typed BuilderError。

## 行为与故障验证

固定25场景，包括补副本及禁用/暂停、已足量、learner参与全部peer计数、
learner不计入多余voter、无目标、临时busy和永久removed、位置优先于负载、
allocator失败、缺失leader、删除follower/leader、多余副本禁用、
删除无需分配ID、unhealthy源仍可删除、无可移除源写pending、
严格位置改善、位置禁用、等价位置和无label时无操作、
临时改善失败不写pending、移动分配失败。

每个成功结果核对描述、kind、优先级、region/epoch和完整有序步骤，逐次
应用实际心跳后校验下一条命令及最终SUCCESS；原始region保持不变。
make-up使用HIGH，其余使用MEDIUM；真实store状态、空间、负载、位置和操作
规划均通过已有core/filter/placement/operator实现。无 mock 调用次数断言。
固定时钟10000秒、分配ID1000，leader count等于store ID避免相等评分的不确定
选择；没有事后忽略字段。无MetaStore持久化调用点，故此处没有存储故障注入。

## 红灯与错误行为识别

[red-api.log](logs/task_042/red-api.log) 保存先注册测试后的缺失模块/API
exit101，也暴露测试StringSlice配置输入类型错误（改用Vec.into）。
green-first/green-2/green-3.log 保留随后的测试修正：typed NoLeader 名称、
OpKind=OpReplica|OpRegion（144），有leader transfer再加OpLeader（400），
以及Go builder的leader score最小值选择。初版transfer到4的期望不正确，
回查builder.go 707–723后以显式唯一leader count固定到store2；未将Rust输出
反向用作oracle。最终冻结fixture独立保存输入和预期。

[mutation.py](logs/task_042/mutation.py) 在实际实现上引入5种错误：
按全部peers删除多余副本、忽略add禁用、make-up降为MEDIUM、丢失pending、
关闭位置改善。同一个task_042_behavior_cases均出现具体行为断言失败exit101，
见 [mutations.json](logs/task_042/mutations.json) 与 mutation-*.log。
脚本finally恢复生产源文件，并运行cargo fmt；最终基线使用恢复后的正确实现。

## 验证命令与资源

[run.py](logs/task_042/run.py) 按1–10顺序atomic mkdir动态领槽，写owner
计划/任务/会话/PID；全部子进程继承绝对CARGO_TARGET_DIR，失败和中断等待子进程
终止后才清理自身锁，保留缓存。所有实际阶段均领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[validate.py](logs/task_042/validate.py) 使用 `RUST_TEST_THREADS=2`，
[commands.json](logs/task_042/commands.json) 保存确切命令、退出码和有效测试数，
stdout/stderr保存在validation-0.log至validation-6.log。


| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_042 -- --nocapture` | 0 | 1通过 / 0失败 / 0忽略；25场景，8操作、17无操作 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 7通过 / 0失败 / 0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1465通过 / 0失败 / 1既有忽略 |

唯一既有忽略为需要独立进程二进制的 `tso_processes`；未新增忽略。
已有server cpu_usage deprecated警告不影响验证结果。

## 只读与提交审计

[audit.json](logs/task_042/audit.json) 记录1101个Go/模块文件逐文件比较后的
聚合哈希、零差异结果、fixture哈希及只读plan哈希。含非忽略untracked文件，
Go文件与模块文件Git状态始终无差异；不运行Go/failpoint/make入口。
两个plan.md均与开始时一致。既有其它任务删除保留并排除出提交；本任务编号文件
获得完成证据后删除，不单独提交该删除。未改依赖、锁文件或其它生产包。
审查本任务全部差异、Markdown相对链接以及暂存差异检查，`git diff --check`
通过。槽位锁已释放、缓存保留。日志只清理行末空白与冗余末尾空行，记录于
[log-normalization.json](logs/task_042/log-normalization.json)，未改变语义输出。
