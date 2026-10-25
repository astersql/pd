# 任务041：异常副本修复检查

状态：已完成。实际实现为
[replica_fault.rs](../../../pkg/schedule/checker/replica_fault.rs)，
[领域说明](../../../pkg/schedule/checker/replica_fault.rs.md)，
[测试](../../../pkg/schedule/checker/task_041_test.rs)。
公开入口为 `ReplicaFaultChecker::check(ReplicaFaultContext, region, allocate)`，
结果包含真实 Operator、typed BuilderError 和 pending cache 写入意图。
只增加 canonical checker crate 模块/测试注册，没有新增依赖或 Cargo.lock 变更。

## 前置与证据性质

已核实批次40提交 `704e7055d`、[task-040](task-040.md) 的通过证据及 canonical
ReplicaStrategy；复用任务39 PauseController 和任务31 PeerOperatorFactory。
真实依赖包括 core RegionInfo/StoresInfo、checker config、store-state/filter/
位置策略、placement 和 operator builder，不以空实现替代。

审查 oracle-index.json，任务41条目为 source_assertion，尚无对应冻结运行输出。
依用户后续授权，Go 运行 oracle/差分不作为准入限制；本任务**没有运行 Go**，
所有期望来自只读源码和现有测试契约推导，**不声称运行差分**。
[fixture](fixtures/task_041.json) 记录完整 source commit、5份 Go 来源 SHA256、
32个显式输入和预期描述/kind/步骤/错误/pending；哈希见
[audit.json](logs/task_041/audit.json)。固定时钟为10000秒，正常分配ID为1000；
down heartbeat 的经过时间精确到纳秒，没有忽略字段或事后归一化。

契约来源：`replica_checker.go` 的 Check/checkDownPeer/checkOfflinePeer/fixPeer；
相邻 TestDownPeer、TestReplaceOfflinePeer、TestOfflineWithOneReplica、
TestFixDownPeer、TestFixOfflinePeer、配置开关与修复优先级测试；步骤顺序来自
`create_operator.go` 和 `builder.go`。这些均为源码依据，不作为 Go 运行记录。

## 行为、故障与范围

- max-down-time 的前1ns、等于、后1ns，未来心跳，DownSeconds 不参与判断。
- down/offline follower、leader、learner；offline 存在任意 learner 时跳过。
- down 优先、各配置开关、暂停、健康区域、首个 down peer 尝试后停止。
- voter 数量判断额外副本；直接删除不需要目标或分配ID；单副本离线 leader 替换。
- 无目标、永久 removed、临时 busy；仅临时拒绝设置 pending；不在库内复制宿主缓存。
- 缺失 store 停止对应阶段，down 缺失后仍允许 offline 阶段；allocator 失败和
  缺失 leader 返回 typed error，无错误操作。
- 对每个成功结果比较描述、kind、HIGH、region/epoch 和完整有序步骤；将冻结
  步骤逐个转换为下一次 RegionInfo 心跳，确认每一步实际返回命令和最终 SUCCESS。
  原始 region 保持不变。classic 非 joint 模式使用真实工厂；joint/placement 的
  内部规则规划复用既有工厂，不把本次场景宣称为其新增全覆盖验证。

没有持久化/I/O 调用点，因此 MetaStore 故障不适用。只控制时间与 ID allocator
边界，busy/removed/missing 使用真实 core store 状态，没有 mock 计数断言、桩、
新 feature 或忽略测试。

## 失败验证与回归识别

[red-api.log](logs/task_041/red-api.log) 为先注册测试后缺失 API 的 exit101；同时
暴露 Duration 测试输入类型错误，改用 canonical 配置字段的 `"30m".parse()`。
初版 fixture 将所有 leader 的 transfer 放在 add 前，且把 down learner 当作 voter
replacement 排序；green-first/green-2.log 暴露对应步骤断言。回查 Go builder 的
planPreferUpStoreAsLeader / planPreferLessLeaderTransfer / peerPlan / planReplace
后修正，依据保存在 [fixture-correction.json](logs/task_041/fixture-correction.json)。
Go 按 node lifecycle 而非 heartbeat 决定加副本前的 leader；不同角色的 learner
移除和 voter 添加不能组合为同角色 replacement。

[mutation.py](logs/task_041/mutation.py) 在真实实现上分别引入5种错误：阈值改为
严格大于、移除 learner guard、按全部 peers 判断额外副本、优先级改为 MEDIUM、
offline 存在时跳过 down。**同一行为测试均因具体断言失败 exit101**，详见
[mutations.json](logs/task_041/mutations.json) 和 mutation-*.log。首次脚本错误地
要求输出包含英文 assertion，未识别 assert! 的 missing operator 输出；实际测试
已失败，脚本改为匹配测试文件 panic 和 FAILED，并重跑全部 mutation。
finally 恢复生产源文件并格式化，最终验证在恢复后的代码上运行。

## 最终验证

[run.py](logs/task_041/run.py) 各阶段按1–10 atomic mkdir 动态领槽，记录任务、
会话和PID，子进程继承绝对 CARGO_TARGET_DIR；退出后只释放自身锁，保留缓存。
所有实际阶段领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[validate.py](logs/task_041/validate.py) 与
[commands.json](logs/task_041/commands.json) 保留确切命令、退出码、测试数；
stdout/stderr 在 validation-0.log 至 validation-6.log。
沿用前置 `RUST_TEST_THREADS=2`。唯一既有忽略为 `tso_processes`（需要独立进程二进制），本任务没有增加忽略；server 的既有 cpu_usage deprecated 警告未影响结果。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_041 -- --nocapture` | 0 | 1通过 / 0失败 / 0忽略；32场景（17个操作、15个无操作） |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 6通过 / 0失败 / 0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1464通过 / 0失败 / 1忽略 |

## 只读审计与差异

Go/模块文件逐文件比较开始/结束哈希与 Git 状态，含非忽略 untracked 文件；
[audit.json](logs/task_041/audit.json) 保存文件数量、聚合哈希与零差异结果。
两个 plan.md 均保持原样；已有其它编号任务删除保持原状并排除出提交。
本任务未运行 Go/failpoint/make，没有新增 Go 或模块文件差异。只提交本任务代码、
测试、fixture、领域说明及验证证据，编号任务完成后删除且不单独提交其删除。
日志的行末空白和冗余末尾空行清理记录在 log-normalization.json；测试语义输出未改。
最终 git diff --check 和暂存差异检查通过；槽位锁已释放，缓存保留。
