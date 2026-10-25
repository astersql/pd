# 任务026：构建器目标校验

状态：已完成。依赖批次25提交 `cdb1dd090` 与 [task-025](task-025.md) 的生产
Operator/OpStep、取消替换和记录证据已核实；当前工作树具备这些 API。
oracle-index 的 controller 并发删除结果不覆盖 builder，未冒用。依用户后续授权，
本任务采用只读 Go 源码及测试契约推导（`source_assertion`，`go_execution=false`）；
未运行 Go builder、未宣称运行差分，两个只读 plan.md 均保持原样。

生产路径：[builder_target.rs](../../../pkg/schedule/operator/builder_target.rs)，
[领域说明](../../../pkg/schedule/operator/builder_target.rs.md)。API 为 Builder、
BuilderContext、BuilderOptions、BuilderError 及 is_allowed_leader_target；局部接线
lib.rs/Cargo.toml，Cargo.lock 仅增加本 crate 的3个既有 workspace 依赖边。
离线锁生成曾更新一个无关远程版本，审查后已恢复原版本与 checksum。

[fixture](fixtures/task_026.json) 固定 source_commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 与 builder.go/builder_test.go 的内容
SHA256，53项输入/预期来自源码分支及相邻 TestNewBuilder、TestRecord、
TestPrepareBuild、TestTargetUnhealthyPeer。TestBuild 含大量后续规划步骤，
本阶段只复用目标校验契约。TestMain/TestOperatorBuilderTestSuite 是入口，
不冒充 Go 运行覆盖。原始固定输入及逐项结果类别均在 fixture，typed error enum
保存分支身份，不比较 protobuf 文本打印，没有忽略状态/目标错误，没有时间随机归一化。

两个有效 Rust 测试（0忽略）：`task_026_behavior_cases` 执行53项表驱动案例，
包括重复 Add/Remove、nil/zero/joint、缺失 store、无 voter、SetPeers/ExpectedRoles、
健康状态、force 和各 Skip 交叉边界；`task_026_multi_target_and_ownership` 验证
多目标排序、重复保留、真实候选准入、borrowed input 不被修改、ID-only no-step。
正常、错误及故障用真实 core/config/fit/filter 值执行；不存在持久化 I/O，不增加 mock。

[red-missing.out](logs/task_026/red-missing.out) 为先注册测试后的 API 缺失失败，退出101。
[red-behavior.py](logs/task_026/red-behavior.py) 分别注入四种错误策略后运行同一聚焦
命令，重复添加/force缺失store/placement检查/角色与witness 的行为断言必须失败，
finally 恢复原生产字节；详见 red.json 与 red-*.out。它们是实际断言失败证据，
不是只靠编译失败。阶段使用 slot-run.py 动态原子领取共享槽位，所有子进程退出后
清理自有锁并保留缓存。

完整基线命令、退出码、有效测试数量和实际 CARGO_TARGET_DIR 见
[validation.json](logs/task_026/validation.json)。workspace 既有 etcd I/O 用前置已保存
二进制，readonly-validation.py 配合拒绝实际 Go 调用的拦截器仅复制已有 fixture，
哈希与复用 provenance 单独保存，不作为本任务 Go oracle。


| 最终命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_026 -- --nocapture` | 0 | 2通过，0失败，0忽略；53项fixture全部成功 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 42通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1405通过，0失败，1既有忽略 |

四种行为变异各退出101，恢复后的生产代码SHA256与最终验证一致，53项固定案例
成功53、失败0；完整输出位于 logs/task_026/。所有阶段动态领取实际槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
验证完成后已释放自有锁并保留缓存。[verification.json](logs/task_026/verification.json)
确认1119项只读清单一致，其中1101个Go/模块文件内容不变，plan.md未改，
`git diff --check`退出0；fixture SHA256为
`ef61cf6cdd5e98b633b75be0cb4c9fbe9fb6209f2642e47b07aca15c1531f0d2`。

已审查实现、测试、依赖边和格式化差异；只提交本任务实际产物，保留其它任务原有
删除。当前只实现目标校验和no-step路径，有效目标变更明确返回PlanningRequired，
等待后续任务加入规划，不将其当作已构建成功。编号任务标记已完成后按授权删除，
稳定证据保留供批次27核验；不修改任何只读plan.md或原主题任务。
