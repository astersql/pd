# 任务029：联合共识步骤规划

前置批次28提交 `1d77a7bc7`，task-028.md 记录47个包测试及 leader
评分验证。已核实 canonical Builder/Planner 与联合共识 OpStep 可用。
本任务通过 `Builder::build_joint` 接入 [生产实现](../../../pkg/schedule/operator/builder_joint.rs)，
复用批次28的 leader 选择，不增加 crate、依赖、feature 或持久化路径。
领域边界见 [builder_joint.rs.md](../../../pkg/schedule/operator/builder_joint.rs.md)。

## 来源与行为

Go 来源 commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`。
当前 builder.go 与 builder_test.go 的 Git blob 分别为
`3cbc8ee37ea7ad6d085c239f0a248431a03cecec`、
`795fbad225393fc8f43a92067549bfc5a255c708`，与该 commit 相同。
[固定 fixture](fixtures/task_029.json) 包含两份源码 SHA256 和21项原始输入/期望输出。
依据 Go prepareBuild、buildStepsWithJointConsensus、execChangePeerV2 与
TestBuild 源码断言推导；oracle-index.json 的运行记录不覆盖本故事。
依据用户后续授权解除 Go 运行准入限制，采用 `source_assertion` /
`go_execution=false`。没有执行 Go oracle，没有运行差分或 Raft 集群执行声明。
TestMain/TestOperatorBuilderTestSuite 仅是入口，未作为本任务运行覆盖。
没有随机或时间字段归一化。

21个场景涵盖 enter 前、joint 中、leave 后切 leader；单项降级、2到1、
降级原 leader 并晋升目标、排序混合增删、纯 learner 增删、普通路径回退、
expected follower 排除、分配新 ID、保留原 peer ID、空步骤、无 voter、
已有 joint 拒绝、无合法 leader、allocator 故障、缺失显式 leader、witness 转换交接。
逐步骤检查目标及顺序、kind、轻量标志、snapshot 来源、peer ID、最终角色和输入所有权。
真实 RegionInfo 回放心跳状态，检查每步 is_finish/conf_ver_changed、leader 资格和
最终非 joint 状态。单成员自动稳定是明确的外部 TiKV 心跳输入模型。
没有存储写入，故障注入只在真实 allocator 边界；错误回写 Builder 并拒绝部分 Operator。

## 失败证据

[red.out](logs/task_029/red.out)：先注册同一 task_029_behavior_cases 测试，
临时将 build_joint 委托普通 build_simple，测试因 transfer-in-joint 的完整步骤
不匹配而失败，退出101（0通过、1失败）。它实际检测普通 PromoteLearner 缺少
ChangePeerV2Enter/Leave 的行为差异，不是缺 API、编译错误或零测试。
[green.out](logs/task_029/green.out) 是最小实现后同一测试通过记录。

## 验证与只读边界

构建阶段通过 [slot-run.py](logs/task_029/slot-run.py) 原子领取共享槽位，
每次从1到10尝试，实际均为槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁 owner 含计划/任务/会话/PID。正常、失败、信号中断均等待命令进程组退出后
仅清理自己的 owner 与锁，保留缓存。

[validation.json](logs/task_029/validation.json) 保存确切命令、退出码和有效测试数，
validation-N.out 保存 stdout/stderr。workspace 的外部 etcd 测试复用槽位中前序
任务保存的现有原生 I/O fixture；Go 拦截器只复制哈希核实的二进制、拒绝实际 Go
调用。[provenance](logs/task_029/fixture-reuse-provenance.json) 和 go-reuse.jsonl
仅是外部 etcd 测试边界证据，与本任务 builder 契约来源无关。

| 验证命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_029 -- --nocapture` | 0 | 1通过、21固定场景、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 48通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1411通过、0失败、1既有忽略 |

[verification.json](logs/task_029/verification.json) 逐一核对1118份受保护文件，
其中1101份Go/模块文件及17份本计划plan.md，SHA256均与任务开始一致；
Go/模块Git状态为空，git diff --check退出0。fixture SHA256：
`a253fca3ee06e935bf784bb72ebffdaa8b4dd621f03afbf813f1440a0ce826c0`。
源码差异审查仅有本任务实现、局部接线、测试、fixture和说明/日志；Go文件、
Cargo.lock、依赖声明及只读plan.md均未改写。已等待全部Cargo子进程结束，
自有槽位锁已清理，缓存保留。使用git-commit技能提交实际变更，未暂存其它
既有任务文件删除。任务29标记已完成后依授权删除编号任务文件；后续依赖由
本证据和提交核实，不自动改写来源主题。
