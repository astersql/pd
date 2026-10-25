# 任务031：副本与领导者操作工厂

前置批次30提交 `a7b2c3022`；已读取 task-030.md 的21场景、49个包测试和
workspace 基线证据，核实 Builder 的普通、联合共识与 witness 规划 API。
本次生产入口 [create_peer.rs](../../../pkg/schedule/operator/create_peer.rs) 的
`PeerOperatorFactory` 提供 Add/Remove/Promote/Demote/Transfer/ForceTransfer/
MovePeer/MoveRegion/MoveLeader/ReplaceLeaderPeer 以及两个 promote/demote + remove
组合入口。全部复用 canonical Builder 和真实 Operator，没有新增依赖、feature、
Cargo.lock 变更或持久化实现；领域边界与宿主能力输入见
[create_peer.rs.md](../../../pkg/schedule/operator/create_peer.rs.md)。

## 来源与固定契约

Go 基准 commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`。
[fixture](fixtures/task_031.json) 固定 create_operator.go、create_operator_test.go、
builder.go 的 SHA256 和48组原始输入/步骤/kind/错误/最终状态。
依用户后续授权，全部标为 `evidence_kind=source_assertion`、`go_execution=false`；
没有 Go 运行 oracle、Go/Rust 运行差分或真实 Raft 集群执行声明，无字段归一化。
既有 oracle-index.json 的运行结果不覆盖这些工厂。

只读核实 `TestCreateOperatorTestSuite/TestCreateTransferLeaderOperator` 的普通、
self、缺失、learner、demoting/incoming 和限流契约；MoveRegion 的 joint 与
without-joint 表，以及 builder.go 的 Add/Remove/Promote/Demote、目标选择与步骤
规划。`TestCreateLeaveJointStateOperatorWithoutFitRules` 验证另一个 leave-joint
工厂，本故事没有借其名字宣称运行覆盖，也没有扩大实现范围。

48场景：25成功、23预期错误。覆盖两个配置分支与 ConfChangeV2 能力门禁、
零ID分配及边界故障、第一错误传播、非法/重复/缺失 peer、learner/voter 角色错误、
origin joint 准入、incoming/demoting 转移、普通/强制的 store 存在/状态/placement/
健康差异、多个候选过滤、move/replace 时的 leader 保护、显式 expected roles、
MoveRegion 旧身份保留与 witness 按输入迭代顺序继承、joint 中途转移 leader。
工厂只组合目标，步骤规划仍由已有 Builder 负责；宿主显式传入版本能力及分配器。

[task_031_test.rs](../../../pkg/schedule/operator/task_031_test.rs) 检查完整步骤与
peer身份、kind、region/epoch/description、分配次数和错误，验证输入所有权。
25项成功操作逐步在真实 RegionInfo 上回放，核对 pending/ack、is_finish、
ConfVerChanged、命令类型、timeout、最终 peers/leader 和 Operator SUCCESS。
Influence 通过真实步骤入口计算，资源值细则复用既有步骤测试。
单项 V2 自动稳定作为显式 TiKV 心跳输入模型；未启动 TiKV。

## 失败与通过

[initial-compile.out](logs/task_031/initial-compile.out) 记录初始 Rust 所有权编译错误，
它不作为行为失败证据。[red.out](logs/task_031/red.out) 为可编译的同一个已注册
`task_031_behavior_cases`：AddPeer 返回 PlanningRequired，与预期成功断言不符；
退出101，0通过、1失败。工厂从旧 `Builder::build` 连接到已验证的
`build_simple`/`build_joint` 后，[green.out](logs/task_031/green.out) 记录迭代及
45项通过，[replay.out](logs/task_031/replay.out) 记录扩展后48项通过。

迭代中两项固定期望经只读源码审查纠正：builder.go 927–953行表明 forced
也必须有目标 store，因此成功场景改用存在但心跳过期的 store，并另测缺失错误；
1191–1202和1264–1270行的 old-peer 排序先于 target-leader，所以普通 MoveLeader
先经过旧 voter 再转到新 peer。没有从 Rust 输出自动生成 fixture；最终状态从
工厂的目标语义独立固定。green.out 还保留一次漏导入编译错误，最终验证无警告。

## 验证

所有 Cargo 命令由 [slot-run.py](logs/task_031/slot-run.py) 依1–10原子领取；
实际均为槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录任务/计划/会话/PID，脚本正常、失败、信号退出都等待自己的子进程后
清理自有锁，缓存保留。确切命令、退出码与有效测试数保存在
[validation.json](logs/task_031/validation.json)，stdout/stderr 为 validation-N.out。

workspace 外部 I/O 测试使用既有原生 etcd fixture，先核验 SHA256，来源见
[provenance](logs/task_031/fixture-reuse-provenance.json)。
[只读入口](logs/task_031/readonly-validation.py) 的 Go 拦截器只允许复制已存在
fixture 二进制，拒绝所有实际 Go 命令，复制记录为 go-reuse.jsonl。
它只是外部 I/O 测试后端，不是本任务 operator oracle。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_031 -- --nocapture` | 0 | 1测试通过；48场景（25成功、23预期错误），0失败/忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 50通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1413通过、0失败、1既有忽略 |

[verification.json](logs/task_031/verification.json) 核对1119份受保护文件：
1101份 Go/模块文件和18份 plan.md 的 SHA256 全部一致，受保护 Git 状态为空，
无新增 Go 文件，源码与指定 Go 基准 commit 无差异。`git diff --check` 退出0，
文档相对链接通过。fixture SHA256：
`d05ade9e9205d4c1c6e350c7e6060acc797939b174fd01c7548f896ade0db0ec`。
已审查差异：工厂生产文件、领域说明、测试/fixture/证据、必要 lib 注册，以及
BuilderError 的 joint-config 门禁错误；没有修改已有规划器、Go、Cargo配置或锁。
全部子进程结束，自有槽位锁已释放，缓存保留。任务已完成，按 git-commit 技能
仅提交本任务实际变更；既有编号任务删除不纳入提交，任务31完成后按授权删除。
